//! The Write sweep's engine: it runs every write operation in the live API description through its generated
//! command, in groups, and checks what each write stored by reading it back through a read command.
//!
//! A group is the resource a path acts on (`risks` for `/api/v1/risks/{id}/comments`). Its writes run in a fixed
//! order, the create first and the delete last. Everything a write acts on is a Sweep record this run created, so
//! the run leaves the Test-bed as it found it. What the description cannot say goes in `write_table`.

use std::fmt::Display;

use serde_json::{Number, Value};

use crate::lookup::read_at;
use crate::operations::{CommandName, Kind, LiveOperation, LiveParameter, placeholders};
use crate::support::Run;
use crate::test_bed::Scenario;
use crate::write_table::{self, Override};

/// The kinds of write in the order a group runs them.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Stage {
    /// A POST to the group's own path.
    Create,
    /// A POST to a path under a record, which has a read of its own, or one that makes a record of another group.
    Add,
    Update,
    /// Any other POST.
    Action,
    SubUpdate,
    SubDelete,
    Delete,
}

/// One write operation, placed in its group and stage.
struct Write<'a> {
    operation: &'a LiveOperation,
    command: &'a CommandName,
    group: &'a str,
    stage: Stage,
    /// An earlier operation the CLI runs under this command's name, so this one has no command of its own.
    shadowed_by: Option<&'a LiveOperation>,
}

/// How a write counts in the summary.
enum Counted {
    /// Ran, and passed every check.
    Run,
    /// Gave the outcome its table entry names.
    Expected,
    Skipped(String),
    Failed(String),
}

/// A record this run created. Its id fills the placeholders and id fields of later writes.
struct Record {
    /// The path the create or add posted to, such as `/api/v1/risks` or `/api/v1/risks/{id}/comments`.
    collection: String,
    id: String,
    deleted: bool,
}

/// A command the binary ran, and what came back.
struct Ran {
    args: Vec<String>,
    run: Run,
}

impl Ran {
    fn failure(&self, check: impl Display) -> String {
        format!(
            "{check}\n    ran `{}`: exit {}, stdout {:?}, stderr {:?}",
            command_line(&self.args),
            self.run.code,
            self.run.stdout,
            self.run.stderr
        )
    }
}

/// How to read a write's record back.
struct ReadBack {
    args: Vec<String>,
    /// The read answers a list, and the record is its item with this id.
    item: Option<String>,
    /// The read is of a record above the one written, which the write must change.
    changes: bool,
}

/// What an entry's `from_read` found.
enum Picked {
    Values(Vec<(&'static str, String)>),
    /// No item qualified, and the entry says that is no failure: why.
    Missing(&'static str),
}

/// The writes in `operations`, run once each, and what is left to report.
struct Sweep<'a> {
    scenario: &'a Scenario,
    operations: &'a [LiveOperation],
    /// Every write, so a write that needs a record of another group can run that group's create.
    writes: &'a [Write<'a>],
    records: Vec<Record>,
    /// The collections of Sweep records the run could not make, with why: every write that needs one is skipped.
    no_record: Vec<(&'static str, &'static str)>,
    /// The ids of the Tasks the Account had when the run began. A write can make Tasks of its own (a review of a
    /// new Asset, an acknowledgement of a new Policy), and while one is left, the record it is about cannot go.
    tasks_before: Vec<String>,
    /// Small files in a temporary folder, which a write with an upload target sends with `--file`. The folder goes
    /// when the sweep does.
    upload: Upload,
    /// The time the run began, in milliseconds, so a value that must be unique (`{n}`) is not one an earlier run left
    /// on a Test-bed this run shares with it.
    started: u128,
}

struct Upload {
    /// For every target but `import`.
    text: String,
    /// For `import`, which takes only a `.zip`. It is not an export, so Compliance's prepare of it fails at once.
    zip: String,
    _folder: tempfile::TempDir,
}

/// Runs every write operation in `operations` and prints the report. Panics when a write failed, a skipped write
/// has no reason, or the counts do not add up to the write operations in `operations`.
pub fn run(scenario: &Scenario, operations: &[LiveOperation]) {
    let writes = writes(operations);
    let expected = operations
        .iter()
        .filter(|operation| {
            operation.is_write()
                && operation
                    .command
                    .as_ref()
                    .is_some_and(CommandName::is_generated)
        })
        .count();
    assert!(
        expected > 0,
        "the live description has no write operation the CLI generates"
    );

    for (index, write) in writes.iter().enumerate() {
        let Some(restore) = write_table::entry(&write.operation.id).put_back_by else {
            continue;
        };
        assert!(
            writes
                .iter()
                .skip(index + 1)
                .any(|later| later.operation.id == restore),
            "{} is put back by {restore}, which does not run after it",
            write.operation.id
        );
    }

    let mut sweep = Sweep {
        scenario,
        operations,
        writes: &writes,
        records: Vec::new(),
        no_record: Vec::new(),
        tasks_before: Vec::new(),
        upload: Upload::new(),
        started: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_millis()),
    };
    sweep.tasks_before = sweep
        .task_ids()
        .expect("the Tasks the Account has before the sweep could not be listed");
    let mut reports: Vec<(&str, Counted)> = Vec::new();
    let mut create_failed: Vec<&str> = Vec::new();
    for write in &writes {
        let counted = if create_failed.contains(&write.group) && write.stage != Stage::Create {
            Counted::Skipped("its group's create failed".to_string())
        } else {
            sweep.execute(write)
        };
        if write.stage == Stage::Create && matches!(counted, Counted::Failed(_)) {
            create_failed.push(write.group);
        }
        reports.push((&write.operation.id, counted));
    }
    let cleanup_failures = sweep.clean_up();

    let count = |wanted: fn(&Counted) -> bool| reports.iter().filter(|(_, c)| wanted(c)).count();
    let (run, checked, skipped, failed) = (
        count(|c| matches!(c, Counted::Run)),
        count(|c| matches!(c, Counted::Expected)),
        count(|c| matches!(c, Counted::Skipped(_))),
        count(|c| matches!(c, Counted::Failed(_))),
    );
    println!(
        "Write sweep: {expected} writes, {run} run, {checked} checked against an expected outcome, {skipped} \
         skipped, {failed} failed"
    );
    let mut failures = Vec::new();
    for (id, counted) in &reports {
        match counted {
            Counted::Skipped(reason) => println!("skipped: {id}: {reason}"),
            Counted::Failed(why) => {
                println!("failed: {id}: {why}");
                failures.push(format!("{id}: {why}"));
            }
            Counted::Run | Counted::Expected => {}
        }
    }
    for why in &cleanup_failures {
        println!("failed: {why}");
        failures.push(why.clone());
    }

    assert_eq!(
        run + checked + skipped + failed,
        expected,
        "the writes run, checked against an expected outcome, skipped, and failed do not add up to the write \
         operations in the live description"
    );
    assert!(
        reports
            .iter()
            .all(|(_, counted)| !matches!(counted, Counted::Skipped(reason) if reason.is_empty())),
        "a skipped write has no reason"
    );
    assert!(
        failures.is_empty(),
        "{} writes failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Every write the CLI generates a command for, grouped by the resource it acts on and ordered as a group runs
/// them: by stage, then by path.
fn writes(operations: &[LiveOperation]) -> Vec<Write<'_>> {
    let mut writes: Vec<Write> = Vec::new();
    for (index, operation) in operations.iter().enumerate() {
        let Some(command) = operation.command.as_ref().filter(|c| c.is_generated()) else {
            continue;
        };
        if !operation.is_write() {
            continue;
        }
        let (group, rest) = split_group(&operation.path);
        let has_read = operations
            .iter()
            .any(|other| other.is_read() && other.path == operation.path);
        // A POST that makes a record of another group's collection adds it, and is read back through that group.
        // One that makes a record of its own group's collection, such as `frameworks.adopt`, is the group's create.
        let made = write_table::entry(&operation.id)
            .made
            .as_ref()
            .filter(|made| made.found_in.is_none());
        let stage = match operation.method.as_str() {
            "POST" if made.is_some_and(|made| made.collection == format!("/api/v1/{group}")) => {
                Stage::Create
            }
            "POST" if made.is_some() => Stage::Add,
            "POST" if !has_read => Stage::Action,
            "POST" if rest.is_empty() => Stage::Create,
            "POST" => Stage::Add,
            "DELETE" if rest.len() == 1 && rest[0].starts_with('{') => Stage::Delete,
            "DELETE" => Stage::SubDelete,
            _ if rest.len() <= 1 => Stage::Update,
            _ => Stage::SubUpdate,
        };
        writes.push(Write {
            operation,
            command,
            group,
            stage,
            shadowed_by: operations[..index]
                .iter()
                .find(|earlier| earlier.command.as_ref() == Some(command)),
        });
    }
    // Groups in the order the description first names them; the sort is stable.
    let mut order: Vec<&str> = Vec::new();
    for write in &writes {
        if !order.contains(&write.group) {
            order.push(write.group);
        }
    }
    writes.sort_by_key(|write| {
        (
            write_table::last_rank(&write.operation.id).map_or(0, |rank| rank + 1),
            !write_table::runs_first(write.group),
            order.iter().position(|group| *group == write.group),
            write.stage,
            write_table::action_rank(write.group, &write.command.action),
            write.operation.path.as_str(),
        )
    });
    writes
}

/// The group a path acts on, and the segments after it: `/api/v1/risks/{id}/comments` is `risks` and
/// `["{id}", "comments"]`.
fn split_group(path: &str) -> (&str, Vec<&str>) {
    let path = path.strip_prefix("/api/v1/").unwrap_or(path);
    let mut segments = path.trim_matches('/').split('/');
    let group = segments.next().unwrap_or_default();
    (group, segments.collect())
}

impl Upload {
    fn new() -> Self {
        let folder = tempfile::tempdir().expect("a temporary folder for the file to upload");
        let file = |name: &str| {
            let path = folder.path().join(name);
            std::fs::write(&path, "A file the Write sweep uploads.\n")
                .expect("write the file to upload");
            path.to_str().expect("a UTF-8 temporary path").to_string()
        };
        Upload {
            text: file("e2e-sweep.txt"),
            zip: file("e2e-sweep.zip"),
            _folder: folder,
        }
    }

    fn file_for(&self, target: &str) -> &str {
        if target == "import" {
            &self.zip
        } else {
            &self.text
        }
    }
}

impl Sweep<'_> {
    /// Runs one write and checks it; every way it can fail is a `Counted::Failed` with the check that failed.
    fn execute(&mut self, write: &Write) -> Counted {
        let entry = write_table::entry(&write.operation.id);
        if let Some(reason) = entry.skip {
            return Counted::Skipped(reason.to_string());
        }
        if let Some(reason) = self.missing_record(write, entry) {
            return Counted::Skipped(reason.to_string());
        }
        self.check(write, entry).unwrap_or_else(Counted::Failed)
    }

    /// Why the write cannot run when it needs a Sweep record the run could not make: it acts on one (its path
    /// starts at the collection), takes the id of one (`refs`), or reads one (`from_read`).
    fn missing_record(&self, write: &Write, entry: &Override) -> Option<&'static str> {
        let under = |path: &str, collection: &str| {
            path.strip_prefix(collection)
                .is_some_and(|rest| rest.starts_with('/'))
        };
        self.no_record
            .iter()
            .find(|(collection, _)| {
                under(&write.operation.path, collection)
                    || entry
                        .refs
                        .iter()
                        .any(|(_, group)| format!("/api/v1/{group}") == *collection)
                    || entry
                        .from_read
                        .iter()
                        .any(|from| under(from.read, collection))
            })
            .map(|(_, reason)| *reason)
    }

    fn check(&mut self, write: &Write, entry: &Override) -> Result<Counted, String> {
        if let Some(earlier) = write.shadowed_by {
            return Err(format!(
                "the CLI runs the earlier {} {} under the name `{}`",
                earlier.method, earlier.path, write.command
            ));
        }
        let mut refs = self.resolve_refs(write, entry)?;
        match self.picks(entry, &refs)? {
            Picked::Values(picks) => refs.extend(picks),
            Picked::Missing(reason) => {
                if let Some(made) = &entry.made {
                    self.no_record.push((made.collection, reason));
                }
                return Ok(Counted::Skipped(reason.to_string()));
            }
        }
        let values = self.path_values(&write.operation.path, &refs, entry)?;
        let creates = matches!(write.stage, Stage::Create | Stage::Add);
        // A create is read back by the id it answers with, so its read is known after the write.
        let read = (!creates).then(|| self.read_back(write, &values, None));
        if matches!(read, Some(None))
            && entry.no_read.is_none()
            && entry.expects.is_none()
            && entry.outcome.is_none()
        {
            return Err(format!(
                "no read to check against, and no table entry for the write at `{}`",
                write.operation.path
            ));
        }
        let before = match (&read, write.stage) {
            (Some(Some(read)), stage)
                if read.changes || matches!(stage, Stage::Update | Stage::SubUpdate) =>
            {
                Some(
                    self.fetch(read)?
                        .ok_or("the record was not found before the write")?,
                )
            }
            _ => None,
        };

        if entry.clears_tasks {
            let failures = self.delete_new_tasks();
            if !failures.is_empty() {
                return Err(failures.join("\n"));
            }
        }
        let (args, sent) = build(write, &values, before.as_ref(), entry, &refs, &self.upload);
        let serial = format!("{}-{}", self.started, self.records.len());
        let args: Vec<String> = args
            .into_iter()
            .map(|arg| arg.replace("{n}", &serial))
            .collect();
        let sent: Vec<(String, Value)> = sent
            .into_iter()
            .map(|(name, value)| (name, with_serial(value, &serial)))
            .collect();
        let ran = Ran {
            run: self
                .scenario
                .run(&args.iter().map(String::as_str).collect::<Vec<_>>()),
            args,
        };

        // Writes that spend the Assistant allowance run on the Launcher's extra funds. An empty allowance is the
        // Test-bed's, not a fault of the command.
        if status_of(&ran.run.stderr) == Some(402) {
            return Err(ran.failure(
                "answered 402: the Test-bed's Assistant allowance for Account 1 ran out; this is not a CLI failure",
            ));
        }
        if let Some(outcome) = &entry.outcome {
            return judge_outcome(&ran, outcome);
        }
        if ran.run.code != 0 {
            return Err(ran.failure("the exit code is not 0"));
        }
        let Ok(answer) = serde_json::from_str::<Value>(&ran.run.stdout) else {
            return Err(ran.failure("stdout is not JSON"));
        };

        if let Some(name) = entry.ends {
            self.end_records(write, entry, name, &refs);
        }
        let made = self.register(write, entry, &ran, &answer, &sent)?;
        if let Some(expects) = &entry.expects {
            self.expect(expects, &refs, entry, &ran)?;
            return Ok(Counted::Run);
        }
        match write.stage {
            Stage::Create | Stage::Add => {
                let id = match made {
                    Some(id) => Some(id),
                    None if entry.singleton => None,
                    None => return Err(ran.failure("the answer has no `id`")),
                };
                let Some(read) = self.read_back(write, &values, id.as_deref()) else {
                    return self.no_read(write, entry, &ran);
                };
                let stored = self.fetch(&read)?.ok_or_else(|| {
                    ran.failure(format!("the new record is not found by `{}`", read.shown()))
                })?;
                compare_sent(&ran, &sent, &stored, entry.partial)?;
            }
            Stage::Update | Stage::SubUpdate => {
                let Some(Some(read)) = &read else {
                    return self.no_read(write, entry, &ran);
                };
                let checked = self
                    .fetch(read)
                    .and_then(|stored| {
                        stored.ok_or_else(|| {
                            ran.failure(format!("the record is not found by `{}`", read.shown()))
                        })
                    })
                    .and_then(|stored| compare_sent(&ran, &sent, &stored, entry.partial));
                // The record the Account has only one of goes back as it was, whether the checks passed or not.
                let put_back = match &before {
                    Some(before)
                        if placeholders(&write.operation.path).is_empty()
                            && entry.put_back_by.is_none() =>
                    {
                        self.put_back(write, read, before, &sent)
                    }
                    _ => Ok(()),
                };
                checked?;
                put_back?;
            }
            Stage::Action | Stage::SubDelete | Stage::Delete => {
                if matches!(write.stage, Stage::SubDelete | Stage::Delete) || entry.removes {
                    self.mark_deleted(write, &values);
                }
                let Some(Some(read)) = &read else {
                    return self.no_read(write, entry, &ran);
                };
                let stored = self.fetch(read)?;
                if entry.removes || !read.changes {
                    if stored.is_some() {
                        return Err(ran.failure(format!(
                            "the record is still found by `{}` after the write removed it",
                            read.shown()
                        )));
                    }
                } else {
                    let stored = stored.ok_or_else(|| {
                        ran.failure(format!("the record is not found by `{}`", read.shown()))
                    })?;
                    if Some(&stored) == before.as_ref() {
                        return Err(ran.failure(format!(
                            "the read-back `{}` is the same as before the write",
                            read.shown()
                        )));
                    }
                }
            }
        }
        Ok(Counted::Run)
    }

    /// Registers the record a write made as a Sweep record: a create or add makes one in its own collection, and
    /// an entry's `made` says where else a write makes one. Returns the id of the record.
    fn register(
        &mut self,
        write: &Write,
        entry: &Override,
        ran: &Ran,
        answer: &Value,
        sent: &[(String, Value)],
    ) -> Result<Option<String>, String> {
        let creates = matches!(write.stage, Stage::Create | Stage::Add);
        if (!creates && entry.made.is_none()) || entry.singleton {
            return Ok(None);
        }
        let collection = entry
            .made
            .as_ref()
            .map_or(write.operation.path.as_str(), |made| made.collection);
        let found = match entry.made.as_ref().and_then(|made| made.found_in) {
            Some((list, field)) => {
                let wanted = sent
                    .iter()
                    .find(|(name, _)| name == field)
                    .map(|(_, value)| value);
                answer[list].as_array().and_then(|items| {
                    items
                        .iter()
                        .find(|item| item.get(field) == wanted)
                        .and_then(|item| item["id"].as_str())
                })
            }
            None => answer["id"].as_str(),
        };
        let id = found.ok_or_else(|| ran.failure("the answer has no `id` of the new record"))?;
        self.records.push(Record {
            collection: collection.to_string(),
            id: id.to_string(),
            deleted: false,
        });
        Ok(Some(id.to_string()))
    }

    /// Marks the Sweep record the write ends, the one whose id it takes under `name`, as deleted. The record is in
    /// the collection of the group `entry.refs` names `name` for, else the write's own.
    fn end_records(
        &mut self,
        write: &Write,
        entry: &Override,
        name: &str,
        refs: &[(&str, String)],
    ) {
        let Some((_, id)) = refs.iter().find(|(wanted, _)| *wanted == name) else {
            return;
        };
        let group = entry
            .refs
            .iter()
            .find(|(wanted, _)| *wanted == name)
            .map_or(write.group, |(_, group)| *group);
        let collection = format!("/api/v1/{group}");
        for record in self
            .records
            .iter_mut()
            .filter(|record| record.collection == collection && record.id == *id)
        {
            record.deleted = true;
        }
    }

    /// The values the entry takes from reads of the Test-bed. When a read has no item that qualifies, and the entry
    /// says why that is no failure, the write is skipped.
    fn picks(&self, entry: &Override, refs: &[(&'static str, String)]) -> Result<Picked, String> {
        let mut picked: Vec<(&'static str, String)> = Vec::new();
        for from in entry.from_read {
            let read = read_at(self.operations, from.read)
                .ok_or_else(|| format!("the description has no read command at `{}`", from.read))?;
            let known: Vec<(&str, String)> = refs.iter().chain(&picked).cloned().collect();
            let mut args = read
                .command
                .as_ref()
                .map(CommandName::args)
                .unwrap_or_default();
            args.extend(self.path_values(from.read, &known, entry)?);
            // A page is as long as the read lets it be, so the item is found past a long start of the list.
            if read.query.iter().any(|parameter| parameter.option == "top") {
                args.extend(["--top".to_string(), "200".to_string()]);
            }
            let found = self.fetch(&ReadBack {
                args,
                item: None,
                changes: false,
            })?;
            let answer =
                found.ok_or_else(|| format!("the read at `{}` found nothing", from.read))?;
            let items = match answer["items"].as_array() {
                Some(items) => items.clone(),
                None => vec![answer],
            };
            let value = items
                .iter()
                .filter(|item| {
                    from.keep.iter().all(|(field, json)| {
                        let wanted: Value =
                            serde_json::from_str(json).expect("the table's `keep` is JSON");
                        item.get(*field).is_some_and(|found| same(&wanted, found))
                    })
                })
                .find_map(|item| match item.pointer(from.pointer)? {
                    Value::String(text) => Some(text.clone()),
                    other => Some(other.to_string()),
                });
            match (value, from.missing) {
                (Some(value), _) => picked.push((from.name, value)),
                (None, Some(reason)) => return Ok(Picked::Missing(reason)),
                (None, None) => {
                    return Err(format!(
                        "no item of `{}` qualifies for `{}`",
                        from.read, from.name
                    ));
                }
            }
        }
        Ok(Picked::Values(picked))
    }

    /// Checks the write with the read its entry names: the answer must hold every field the entry expects.
    fn expect(
        &self,
        expects: &write_table::Expects,
        refs: &[(&str, String)],
        entry: &Override,
        ran: &Ran,
    ) -> Result<(), String> {
        let read = read_at(self.operations, expects.read)
            .ok_or_else(|| format!("the description has no read command at `{}`", expects.read))?;
        let mut args = read
            .command
            .as_ref()
            .map(CommandName::args)
            .unwrap_or_default();
        args.extend(self.path_values(expects.read, refs, entry)?);
        let read = ReadBack {
            args,
            item: None,
            changes: false,
        };
        let stored = self
            .fetch(&read)?
            .ok_or_else(|| ran.failure(format!("the record is not found by `{}`", read.shown())))?;
        for (field, json) in expects.fields {
            let wanted: Value =
                serde_json::from_str(json).expect("the table's `fields` value is JSON");
            if !stored.get(*field).is_some_and(|found| same(&wanted, found)) {
                return Err(ran.failure(format!("`{}` has no `{field}` of {json}", read.shown())));
            }
        }
        Ok(())
    }

    /// Marks the record a delete ended, or a write that removes its record, as deleted.
    fn mark_deleted(&mut self, write: &Write, values: &[String]) {
        let path = write.operation.path.as_str();
        // A delete's record is the one its path ends at; any other write removes the one its path starts at.
        let ended = if matches!(write.stage, Stage::Action) {
            path.split_once("/{").zip(values.first())
        } else {
            path.rsplit_once('/').zip(values.last())
        };
        if let Some(((collection, _), id)) = ended {
            for record in self
                .records
                .iter_mut()
                .filter(|record| &record.id == id && record.collection == collection)
            {
                record.deleted = true;
            }
        }
    }

    /// A write with nothing to read it back with passes on its exit code and JSON when its table entry says why.
    fn no_read(&self, write: &Write, entry: &Override, ran: &Ran) -> Result<Counted, String> {
        match entry.no_read {
            Some(_) => Ok(Counted::Run),
            None => Err(ran.failure(format!(
                "no read to check against, and no table entry for the write at `{}`",
                write.operation.path
            ))),
        }
    }

    /// The id of the Sweep record of every group the entry points at, by the name it takes: a Sweep record is made
    /// through the group's own create when none is left. An entry for a write on a `fresh` record also gets that
    /// record's id as `id`, the placeholder of the record the write acts on.
    fn resolve_refs(
        &mut self,
        write: &Write,
        entry: &Override,
    ) -> Result<Vec<(&'static str, String)>, String> {
        let mut refs = Vec::new();
        for (name, group) in entry.refs {
            let collection = format!("/api/v1/{group}");
            let live = self
                .records
                .iter()
                .rev()
                .find(|record| record.collection == collection && !record.deleted)
                .map(|record| record.id.clone());
            refs.push((*name, live.map_or_else(|| self.create_record(group), Ok)?));
        }
        if entry.fresh {
            let id = self.create_record(write.group)?;
            // Kept behind the group's own record, so later writes of the group still find that one.
            let record = self.records.pop().expect("the create left a Sweep record");
            self.records.insert(0, record);
            // A write that ends its record takes the id under the name it ends it by.
            if let Some(name) = entry.ends {
                refs.push((name, id.clone()));
            }
            refs.push(("id", id));
        }
        Ok(refs)
    }

    /// Makes a Sweep record of `collection` (a path after `/api/v1/`, such as `people`) through its create or add,
    /// and returns its id.
    fn create_record(&mut self, collection: &str) -> Result<String, String> {
        let writes = self.writes;
        let path = format!("/api/v1/{collection}");
        // A create posts to the collection's path, or is a write the table says makes a record of the collection.
        let create = writes
            .iter()
            .find(|write| {
                matches!(write.stage, Stage::Create | Stage::Add)
                    && (write.operation.path == path
                        || write_table::entry(&write.operation.id)
                            .made
                            .as_ref()
                            .is_some_and(|made| made.collection == path && made.found_in.is_none()))
            })
            .ok_or_else(|| format!("no create command makes a Sweep record of `{collection}`"))?;
        let made = self
            .check(create, write_table::entry(&create.operation.id))
            .map_err(|why| {
                format!("the Sweep record of `{collection}` could not be made: {why}")
            })?;
        if let Counted::Skipped(reason) = made {
            return Err(format!(
                "the Sweep record of `{collection}` could not be made: {reason}"
            ));
        }
        self.records
            .last()
            .map(|record| record.id.clone())
            .ok_or_else(|| format!("the create of `{collection}` left no Sweep record"))
    }

    /// Writes back, through the same command, what the record held before `write` changed it, and checks that
    /// `read` shows it again. `before` is that read taken before the write.
    fn put_back(
        &self,
        write: &Write,
        read: &ReadBack,
        before: &Value,
        sent: &[(String, Value)],
    ) -> Result<(), String> {
        let mut args = write.command.args();
        let mut held = Vec::new();
        for (name, _) in sent {
            let parameter = write
                .operation
                .body_fields
                .iter()
                .find(|field| field.name == *name)
                .ok_or_else(|| {
                    format!("cannot put back `{name}`: the command has no option for it")
                })?;
            let value = before
                .get(name)
                .filter(|value| !value.is_null())
                .ok_or_else(|| {
                    format!(
                        "cannot put back `{name}`: the read before the write has no value for it"
                    )
                })?;
            let items = match value {
                Value::Array(items) if parameter.array => items.iter().collect(),
                single => vec![single],
            };
            for item in items {
                args.push(format!("--{}", parameter.option));
                args.push(option_text(item));
            }
            held.push((name.clone(), value.clone()));
        }
        let run = self
            .scenario
            .run(&args.iter().map(String::as_str).collect::<Vec<_>>());
        let ran = Ran { args, run };
        if ran.run.code != 0 {
            return Err(ran.failure("putting back the value read before the write failed"));
        }
        let stored = self.fetch(read)?.ok_or_else(|| {
            ran.failure(format!(
                "the record is not found by `{}` after the put-back",
                read.shown()
            ))
        })?;
        compare_sent(&ran, &held, &stored, &[])
            .map_err(|why| format!("after putting back the value read before the write: {why}"))
    }

    /// A value for every placeholder in `path`, in order: the id the entry points it at, else the entry's fixed
    /// value, else the id of the Sweep record the path before it names.
    fn path_values(
        &self,
        path: &str,
        refs: &[(&str, String)],
        entry: &Override,
    ) -> Result<Vec<String>, String> {
        placeholders(path)
            .into_iter()
            .map(|name| {
                if let Some((_, id)) = refs.iter().find(|(wanted, _)| *wanted == name) {
                    return Ok(id.clone());
                }
                if let Some((_, texts)) = entry.fixed.iter().find(|(wanted, _)| *wanted == name) {
                    return Ok(texts[0].to_string());
                }
                let (collection, _) = path.split_once(&format!("/{{{name}}}")).unwrap_or_default();
                self.records
                    .iter()
                    .rev()
                    .find(|record| record.collection == collection && !record.deleted)
                    .map(|record| record.id.clone())
                    .ok_or_else(|| {
                        format!("no Sweep record created at `{collection}` to fill `{{{name}}}`")
                    })
            })
            .collect()
    }

    /// The read command that shows what `write` stored, with the arguments that name the record. `new_id` is the
    /// id a create or add answered with. `None` when the description has no read to check it against.
    fn read_back(
        &self,
        write: &Write,
        values: &[String],
        new_id: Option<&str>,
    ) -> Option<ReadBack> {
        if write_table::entry(&write.operation.id).no_read.is_some() {
            return None;
        }
        let path = write.operation.path.as_str();
        let get = |wanted: &str| read_at(self.operations, wanted);
        let command_args = |read: &LiveOperation, values: &[String]| {
            let mut args = read.command.as_ref()?.args();
            args.extend(values.iter().cloned());
            Some(args)
        };
        // The nearest path above the write's that has a read: the record the write acts on, which must change.
        let record_above = || {
            let mut parent = path;
            loop {
                parent = parent.rsplit_once('/')?.0;
                if let Some(read) = get(parent) {
                    let count = placeholders(parent).len();
                    return Some(ReadBack {
                        args: command_args(read, values.get(..count)?)?,
                        item: None,
                        changes: true,
                    });
                }
            }
        };
        match write.stage {
            Stage::Create | Stage::Add => {
                // A record the path names alone, such as a Framework's statement, is read at the write's own path.
                if write_table::entry(&write.operation.id).singleton {
                    return Some(ReadBack {
                        args: command_args(get(path)?, values)?,
                        item: None,
                        changes: false,
                    });
                }
                let id = new_id?;
                // A record of another group is read through that group's own item read, which needs no placeholder.
                let (path, values) = match &write_table::entry(&write.operation.id).made {
                    Some(made) => (made.collection, &[][..]),
                    None => (path, values),
                };
                let item = self.operations.iter().find(|operation| {
                    operation.is_read()
                        && operation.command.is_some()
                        && is_item_path(&operation.path, path)
                });
                match item {
                    Some(item) => {
                        let mut values = values.to_vec();
                        values.push(id.to_string());
                        Some(ReadBack {
                            args: command_args(item, &values)?,
                            item: None,
                            changes: false,
                        })
                    }
                    None => Some(ReadBack {
                        args: command_args(get(path)?, values)?,
                        item: Some(id.to_string()),
                        changes: false,
                    }),
                }
            }
            Stage::Update | Stage::SubUpdate | Stage::Delete | Stage::SubDelete => {
                if let Some(read) = get(path) {
                    return Some(ReadBack {
                        args: command_args(read, values)?,
                        item: None,
                        changes: false,
                    });
                }
                // The item has no read of its own: the list above it holds it, past any placeholders between.
                let mut list = path;
                while list
                    .rsplit_once('/')
                    .is_some_and(|(_, last)| last.starts_with('{'))
                {
                    list = list.rsplit_once('/')?.0;
                }
                if let Some(read) = get(list).filter(|_| list != path) {
                    return Some(ReadBack {
                        args: command_args(read, values.get(..placeholders(list).len())?)?,
                        item: values.last().cloned(),
                        changes: false,
                    });
                }
                matches!(write.stage, Stage::SubUpdate | Stage::SubDelete)
                    .then(record_above)
                    .flatten()
            }
            Stage::Action => record_above(),
        }
    }

    /// Runs `read`: the record it names, or `None` when Compliance answers 404 or the list has no such item.
    fn fetch(&self, read: &ReadBack) -> Result<Option<Value>, String> {
        let run = self
            .scenario
            .run(&read.args.iter().map(String::as_str).collect::<Vec<_>>());
        let ran = Ran {
            args: read.args.clone(),
            run,
        };
        match ran.run.code {
            0 => {}
            1 if status_of(&ran.run.stderr) == Some(404) => return Ok(None),
            _ => return Err(ran.failure("the read-back failed")),
        }
        let Ok(answer) = serde_json::from_str::<Value>(&ran.run.stdout) else {
            return Err(ran.failure("the read-back printed no JSON"));
        };
        Ok(match &read.item {
            Some(id) => find_by_id(&answer, id).cloned(),
            None => Some(answer),
        })
    }

    /// Deletes every Sweep record that is not deleted yet and whose group has a delete, newest first, and again
    /// while a pass deletes something: a record another one points at can go once that one is gone. Returns what
    /// the last pass could not delete.
    fn clean_up(&mut self) -> Vec<String> {
        let mut failures = self.delete_new_tasks();
        loop {
            let (deleted, pass) = self.delete_records();
            if deleted == 0 || pass.is_empty() {
                failures.extend(pass);
                return failures;
            }
        }
    }

    /// The ids of every Task of the Account, page by page.
    fn task_ids(&self) -> Result<Vec<String>, String> {
        let mut ids = Vec::new();
        let mut skip = 0u64;
        loop {
            let page = skip.to_string();
            let run = self
                .scenario
                .run(&["tasks", "list", "--top", "200", "--skip", &page]);
            let listed: Value = serde_json::from_str(&run.stdout)
                .map_err(|_| format!("`compliance tasks list` gave {:?}", run.stdout))?;
            let items = listed["items"]
                .as_array()
                .ok_or("the Task list has no items")?;
            ids.extend(
                items
                    .iter()
                    .filter_map(|item| item["id"].as_str().map(String::from)),
            );
            match listed["nextOffset"].as_u64() {
                Some(next) => skip = next,
                None => return Ok(ids),
            }
        }
    }

    /// Deletes the Tasks that were made since the run began. Returns what went wrong.
    fn delete_new_tasks(&mut self) -> Vec<String> {
        let ids = match self.task_ids() {
            Ok(ids) => ids,
            Err(why) => return vec![format!("cleanup: {why}")],
        };
        let mut failures = Vec::new();
        for id in ids.iter().filter(|id| !self.tasks_before.contains(id)) {
            let run = self.scenario.run(&["tasks", "delete", id]);
            if run.code == 0 {
                for record in &mut self.records {
                    if record.collection == "/api/v1/tasks" && record.id == *id {
                        record.deleted = true;
                    }
                }
            } else {
                let args = vec!["tasks".to_string(), "delete".to_string(), id.clone()];
                failures.push(Ran { args, run }.failure(format!(
                    "cleanup: could not delete the Task {id}, which the run made"
                )));
            }
        }
        failures
    }

    /// One pass of `clean_up`: how many records it deleted, and what went wrong for the others.
    fn delete_records(&mut self) -> (usize, Vec<String>) {
        let (mut deleted, mut failures) = (0, Vec::new());
        for index in (0..self.records.len()).rev() {
            let record = &self.records[index];
            if record.deleted || record.collection.contains('{') {
                continue;
            }
            // A collection without a delete says which write ends its records.
            let ender = write_table::ended_by(&record.collection);
            let Some(command) = self
                .operations
                .iter()
                .find(|operation| match ender {
                    Some(id) => operation.id == id,
                    None => {
                        operation.method == ureq::http::Method::DELETE
                            && is_item_path(&operation.path, &record.collection)
                    }
                })
                .and_then(|operation| operation.command.as_ref())
            else {
                continue;
            };
            let mut args = command.args();
            args.push(record.id.clone());
            let run = self
                .scenario
                .run(&args.iter().map(String::as_str).collect::<Vec<_>>());
            let ran = Ran { args, run };
            if ran.run.code == 0 {
                self.records[index].deleted = true;
                deleted += 1;
            } else {
                failures.push(ran.failure(format!(
                    "cleanup: could not delete the Sweep record {} of `{}`",
                    self.records[index].id, self.records[index].collection
                )));
            }
        }
        (deleted, failures)
    }
}

impl ReadBack {
    fn shown(&self) -> String {
        command_line(&self.args)
    }
}

/// `compliance <args>` as one line, an empty argument or one with a space in quotes.
fn command_line(args: &[String]) -> String {
    let quoted = args.iter().map(|arg| {
        if arg.is_empty() || arg.contains(' ') {
            format!("{arg:?}")
        } else {
            arg.clone()
        }
    });
    format!("compliance {}", quoted.collect::<Vec<_>>().join(" "))
}

/// Whether `path` is the path of one item of the collection at `collection`: `/api/v1/risks/{id}` of
/// `/api/v1/risks`.
fn is_item_path(path: &str, collection: &str) -> bool {
    path.strip_prefix(collection)
        .and_then(|rest| rest.strip_prefix('/'))
        .is_some_and(|name| name.starts_with('{') && name.ends_with('}') && !name.contains('/'))
}

/// The `status` of an error printed as JSON on stderr.
fn status_of(stderr: &str) -> Option<u64> {
    serde_json::from_str::<Value>(stderr).ok()?["status"].as_u64()
}

/// An expected outcome: the exit code, JSON where the command prints it, and for exit 1 the status.
fn judge_outcome(ran: &Ran, outcome: &write_table::Outcome) -> Result<Counted, String> {
    if ran.run.code != outcome.code {
        return Err(ran.failure(format!(
            "the exit code is not the expected {}",
            outcome.code
        )));
    }
    // A failure is printed to stderr, anything else to stdout.
    let (printed, stream) = match outcome.code {
        1 => (&ran.run.stderr, "stderr"),
        _ => (&ran.run.stdout, "stdout"),
    };
    if serde_json::from_str::<Value>(printed).is_err() {
        return Err(ran.failure(format!("{stream} is not JSON")));
    }
    if let Some(status) = outcome.status
        && status_of(&ran.run.stderr) != Some(u64::from(status))
    {
        return Err(ran.failure(format!("the status is not the expected {status}")));
    }
    Ok(Counted::Expected)
}

/// The command line for `write` and the body fields it sends, with the values the rules make from the
/// description: the table's value for a field, else none for a field whose value no schema can make, else one
/// value of the field's type. An update picks values that differ from the ones in `before`.
fn build(
    write: &Write,
    values: &[String],
    before: Option<&Value>,
    entry: &Override,
    refs: &[(&str, String)],
    upload: &Upload,
) -> (Vec<String>, Vec<(String, Value)>) {
    let operation = write.operation;
    let mut args = write.command.args();
    args.extend(values.iter().cloned());
    // The upload id of a field with an upload target is what `--file` makes.
    if let Some(target) = operation
        .body_fields
        .iter()
        .find_map(|field| field.upload_target.as_deref())
    {
        args.extend(["--file".to_string(), upload.file_for(target).to_string()]);
    }
    let mut sent = Vec::new();
    if let Some(template) = entry.body {
        let text = refs.iter().fold(template.to_string(), |text, (name, id)| {
            text.replace(&format!("{{{name}}}"), id)
        });
        let Ok(Value::Object(fields)) = serde_json::from_str(&text) else {
            panic!("the table's body for {} is not a JSON object", operation.id);
        };
        args.push("--body".to_string());
        args.push(text);
        sent.extend(fields);
    }
    for (parameter, in_body) in operation
        .query
        .iter()
        .map(|parameter| (parameter, false))
        .chain(
            operation
                .body_fields
                .iter()
                .map(|parameter| (parameter, true)),
        )
    {
        // An optional query parameter is not a body field, and nothing reads it back.
        if !in_body && !parameter.required {
            continue;
        }
        let current = before
            .and_then(|record| record.get(&parameter.name))
            .and_then(|value| {
                if parameter.array {
                    value.get(0)
                } else {
                    Some(value)
                }
            });
        // A `<x>Kind` field only means something beside the `<x>Id` the sweep leaves out: alone, the Kind is refused.
        let kind_of_missing_id = parameter.name.strip_suffix("Kind").is_some_and(|prefix| {
            let id = format!("{prefix}Id");
            operation.body_fields.iter().any(|field| field.name == id)
                && !refs.iter().any(|(name, _)| *name == id)
        });
        if kind_of_missing_id {
            continue;
        }
        let Some(value) = value_for(&operation.id, parameter, current, entry, refs) else {
            continue;
        };
        args.push(format!("--{}", parameter.option));
        args.push(option_text(&value));
        if in_body {
            let value = if parameter.array {
                Value::Array(vec![value])
            } else {
                value
            };
            sent.push((parameter.name.clone(), value));
        }
    }
    (args, sent)
}

/// The value to send for `parameter` (one item of it, for an array), or `None` to leave it out. `current` is what
/// the record holds now, so an update sends something else.
fn value_for(
    operation_id: &str,
    parameter: &LiveParameter,
    current: Option<&Value>,
    entry: &Override,
    refs: &[(&str, String)],
) -> Option<Value> {
    if let Some((_, id)) = refs.iter().find(|(name, _)| *name == parameter.name) {
        return typed(parameter.kind, id);
    }
    let fixed = entry.fixed.iter().find(|(name, _)| *name == parameter.name);
    let left_out = fixed.is_none()
        && (entry.omit.contains(&parameter.name.as_str())
        // No value made from the schema alone points at a real record, or fills an upload.
        || parameter.format.as_deref() == Some("uuid")
        || parameter.upload_target.is_some()
        || parameter.kind == Kind::Json);
    if left_out {
        return None;
    }
    let candidates: Vec<Value> = if let Some((_, texts)) = fixed {
        texts
            .iter()
            .filter_map(|text| typed(parameter.kind, text))
            .collect()
    } else if !parameter.choices.is_empty() {
        parameter
            .choices
            .iter()
            .filter_map(|choice| typed(parameter.kind, choice))
            .collect()
    } else {
        match (parameter.kind, parameter.format.as_deref()) {
            (Kind::String, Some("date")) => vec!["2026-03-15".into(), "2026-03-16".into()],
            (Kind::String, Some("date-time")) => {
                vec!["2026-03-15T10:00:00Z".into(), "2026-03-16T10:00:00Z".into()]
            }
            (Kind::String, _) => {
                let text = format!("E2E sweep {operation_id} {}", parameter.name);
                vec![text.clone().into(), format!("{text} changed").into()]
            }
            (Kind::Integer, _) => vec![2.into(), 3.into()],
            (Kind::Number, _) => vec![2.5.into(), 3.5.into()],
            (Kind::Boolean, _) => vec![true.into(), false.into()],
            (Kind::Json, _) => Vec::new(),
        }
    };
    // The next candidate after the one the record holds, so the value changes.
    let next = current
        .and_then(|current| {
            candidates
                .iter()
                .position(|candidate| same(candidate, current))
        })
        .map_or(0, |position| (position + 1) % candidates.len());
    candidates.into_iter().nth(next)
}

/// `value` with each `{n}` in its texts replaced by `serial`.
fn with_serial(value: Value, serial: &str) -> Value {
    match value {
        Value::String(text) => Value::String(text.replace("{n}", serial)),
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|item| with_serial(item, serial))
                .collect(),
        ),
        other => other,
    }
}

/// `value` as its option takes it on the command line: a text as itself, anything else as JSON.
fn option_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// `text`, as the option of a parameter of `kind` takes it.
fn typed(kind: Kind, text: &str) -> Option<Value> {
    match kind {
        Kind::String => Some(Value::String(text.to_string())),
        Kind::Integer | Kind::Number => text.parse::<Number>().ok().map(Value::Number),
        Kind::Boolean => text.parse::<bool>().ok().map(Value::Bool),
        Kind::Json => serde_json::from_str(text).ok(),
    }
}

/// Every value sent that the read-back also has at its top level is the value sent. A field in `partial` is a list
/// the write merges into the stored one, so each item sent is an item of the stored list.
fn compare_sent(
    ran: &Ran,
    sent: &[(String, Value)],
    stored: &Value,
    partial: &[&str],
) -> Result<(), String> {
    for (name, value) in sent {
        let Some(found) = stored.get(name) else {
            continue;
        };
        let holds = match (partial.contains(&name.as_str()), value, found) {
            (true, Value::Array(items), Value::Array(stored_items)) => items
                .iter()
                .all(|item| stored_items.iter().any(|stored| same(item, stored))),
            _ => same(value, found),
        };
        if !holds {
            return Err(ran.failure(format!(
                "the read-back's `{name}` is {found}, but the write sent {value}"
            )));
        }
    }
    Ok(())
}

/// Two JSON values are the same: numbers by value, dates and date-times as the same instant, arrays item by item.
fn same(sent: &Value, found: &Value) -> bool {
    match (sent, found) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Value::String(a), Value::String(b)) => {
            a == b || instant(a).is_some_and(|a| instant(b) == Some(a))
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(a, b))
        }
        // An object holds every field sent; the record may carry more.
        (Value::Object(a), Value::Object(b)) => a
            .iter()
            .all(|(name, value)| b.get(name).is_some_and(|found| same(value, found))),
        _ => sent == found,
    }
}

/// The object with `id` anywhere inside `value`: an item of a page, or of a list inside a record.
fn find_by_id<'a>(value: &'a Value, id: &str) -> Option<&'a Value> {
    match value {
        Value::Object(fields) => {
            if value["id"] == id {
                return Some(value);
            }
            fields.values().find_map(|inner| find_by_id(inner, id))
        }
        Value::Array(items) => items.iter().find_map(|inner| find_by_id(inner, id)),
        _ => None,
    }
}

/// A date (`2026-03-15`, midnight UTC) or a date-time (`2026-03-15T10:00:00.5+02:00`) as seconds since 1970 and
/// nanoseconds, or `None` for any other text.
fn instant(text: &str) -> Option<(i64, u32)> {
    let (date, time) = match text.split_once('T') {
        Some((date, time)) => (date, Some(time)),
        None => (text, None),
    };
    let number = |part: Option<&str>, length: usize| {
        part.filter(|part| part.len() == length && part.bytes().all(|b| b.is_ascii_digit()))?
            .parse::<i64>()
            .ok()
    };
    let mut parts = date.split('-');
    let (year, month, day) = (
        number(parts.next(), 4)?,
        number(parts.next(), 2)?,
        number(parts.next(), 2)?,
    );
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let mut seconds = days_from_civil(year, month, day) * 86_400;
    let mut nanos = 0;
    if let Some(time) = time {
        let (clock, offset) = match time.strip_suffix('Z') {
            Some(clock) => (clock, 0),
            None => {
                let at = time.rfind(['+', '-'])?;
                let sign = if time[at..].starts_with('+') { 1 } else { -1 };
                let mut zone = time[at + 1..].split(':');
                let hours = number(zone.next(), 2)?;
                let minutes = number(zone.next(), 2)?;
                (&time[..at], sign * (hours * 3600 + minutes * 60))
            }
        };
        let (clock, fraction) = clock.split_once('.').unwrap_or((clock, ""));
        let mut clock = clock.split(':');
        let (hours, minutes, secs) = (
            number(clock.next(), 2)?,
            number(clock.next(), 2)?,
            number(clock.next(), 2)?,
        );
        seconds += hours * 3600 + minutes * 60 + secs - offset;
        if !fraction.bytes().all(|b| b.is_ascii_digit()) || fraction.len() > 9 {
            return None;
        }
        nanos = format!("{fraction:0<9}").parse().ok()?;
    }
    Some((seconds, nanos))
}

/// Days from 1970-01-01 to the given date in the proleptic Gregorian calendar.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}
