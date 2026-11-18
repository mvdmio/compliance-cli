//! What the Write sweep cannot read from the live description: one entry per operation id.
//!
//! An operation with no entry gets a body built from the description alone. An entry goes here only where that
//! body is wrong, with the reason beside it. Keep one group's entries together.

/// The outcome a write gives on a Test-bed other than success.
pub struct Outcome {
    pub code: i32,
    /// For exit 1, the `status` in the error JSON on stderr.
    pub status: Option<u16>,
}

/// Where a write's answer holds the record it made, when that is not the write's own collection and answer `id`.
pub struct Made {
    /// The path of the group the record belongs to, such as `/api/v1/findings`: its create makes records there, and
    /// the end-of-run cleanup deletes this one through that group's delete.
    pub collection: &'static str,
    /// The record is an item of a list in the answer, found by this field of the list's items having the value the
    /// write sent for it: `(list, field)`. Without it the record is the answer itself.
    pub found_in: Option<(&'static str, &'static str)>,
}

/// A value the write takes from a read of the Test-bed: the first item of the read's `items` (the answer itself
/// when it has none) whose fields hold every value in `keep`, and in it the value at `pointer`.
pub struct FromRead {
    /// What the value is for: a path placeholder, an option, or a `{name}` in `body`.
    pub name: &'static str,
    /// The path of a read in the description. Its placeholders take the ids of Sweep records.
    pub read: &'static str,
    /// Fields the item must hold, as `(field, JSON)`.
    pub keep: &'static [(&'static str, &'static str)],
    /// A JSON pointer to the value in the item, such as `/artifact/id`.
    pub pointer: &'static str,
    /// When no item qualifies, the write is skipped for this reason, and so is every write that needs the Sweep
    /// record it makes. Without it, no item is a failure.
    pub missing: Option<&'static str>,
}

/// What a read must show after a write, in place of the rules' read-back.
pub struct Expects {
    /// The path of a read in the description. Its placeholders take the ids of Sweep records.
    pub read: &'static str,
    /// Fields the read's answer must hold, as `(field, JSON)`.
    pub fields: &'static [(&'static str, &'static str)],
}

/// What one write needs beyond the rules.
pub struct Override {
    pub id: &'static str,
    /// Body fields left out, because a value made from the schema alone is refused.
    pub omit: &'static [&'static str],
    /// Body fields sent with one of these values in place of the rule's, as the option takes it on the command
    /// line. An update sends the one after the value the record holds, so give two where the field is updated.
    /// A path placeholder takes the first of its values. `{n}` in a value is a number no earlier record of this run
    /// or of an earlier run on the same Test-bed used, for a field that must be unique.
    pub fixed: &'static [(&'static str, &'static [&'static str])],
    /// Names the write takes the id of a Sweep record for, with the collection of the record (a path after
    /// `/api/v1/`, such as `people` or `organisation-profile/comments`): a body field or query parameter, a path
    /// placeholder, or a `{name}` in `body`. One name does all three, so use none a path placeholder of the write
    /// has, such as `id`. The collection's own create or add makes it when none is left.
    pub refs: &'static [(&'static str, &'static str)],
    /// The body as JSON, sent with `--body`, for a body no field option can carry. Its top-level fields are checked
    /// like the fields sent through options, so none of them is also a field option.
    pub body: Option<&'static str>,
    /// The write ends the life of the record it acts on, so it acts on a Sweep record of its own, made through the
    /// group's create, and the group's other writes keep the record they share.
    pub fresh: bool,
    /// The write makes a record of another group, or one the answer holds inside a list.
    pub made: Option<Made>,
    /// The write removes the record it acts on, which reads back as not found afterwards.
    pub removes: bool,
    /// Compliance opened Tasks for the record, and refuses the write while one is open: the Tasks the run made are
    /// deleted first.
    pub clears_tasks: bool,
    /// The write is not run, and why.
    pub skip: Option<&'static str>,
    /// There is no read to check the write against, and why (a read that cannot tell the write's effect counts): it
    /// passes on its exit code and JSON alone.
    pub no_read: Option<&'static str>,
    /// The write must give this outcome instead of a success that changes something.
    pub outcome: Option<Outcome>,
    /// Body fields that are lists the write merges into the stored one, so the read-back holds more items than the
    /// write sent: each item sent must be one of them.
    pub partial: &'static [&'static str],
    /// A write to a record the Account has only one of is put back by sending the value read before it. Where that
    /// would not undo it, this names the write that does, which the sweep checks runs after this one.
    pub put_back_by: Option<&'static str>,
    /// Values the write takes from a read of the Test-bed, where a Sweep record's id is not enough.
    pub from_read: &'static [FromRead],
    /// A read of a record the description gives no item read for, or where the rules' read-back cannot tell the
    /// effect, which the write is checked with.
    pub expects: Option<Expects>,
    /// The write ends the Sweep record whose id it takes under this name (a path placeholder or a `refs` name).
    pub ends: Option<&'static str>,
    /// The write makes the one record its path names, such as a Framework's statement, which has no id of its own.
    pub singleton: bool,
}

const NONE: Override = Override {
    id: "",
    omit: &[],
    fixed: &[],
    refs: &[],
    body: None,
    fresh: false,
    made: None,
    removes: false,
    clears_tasks: false,
    skip: None,
    no_read: None,
    outcome: None,
    partial: &[],
    put_back_by: None,
    from_read: &[],
    expects: None,
    ends: None,
    singleton: false,
};

/// Two web addresses, for a link field that refuses other text; an update sends the one it does not hold.
const URLS: &[&str] = &[
    "https://example.com/e2e-sweep-1",
    "https://example.com/e2e-sweep-2",
];

/// A Person's email is unique, and a run makes several Persons.
const EMAILS: &[&str] = &["e2e-sweep-{n}@example.com"];

const COUNTRIES: &[&str] = &["NL", "DE"];

const WEBSITES: &[&str] = &[
    "https://e2e-sweep-1.example.com",
    "https://e2e-sweep-2.example.com",
];

/// A Customer needs one security contact, which no field option can carry.
const SECURITY_CONTACTS: &str =
    r#"{"securityContacts":[{"name":"E2E sweep contact","email":"e2e-sweep@example.com"}]}"#;

const IDENTITY_EMAILS: &[&str] = &["e2e-sweep-identity-{n}@example.com"];

/// A target of an Event, and a link of Evidence: a `type` and an `id`, which would clash with the path's `id` as
/// options.
const TARGET_RISK: &str = r#"{"type":"Risk","id":"{riskId}"}"#;
const TARGET_TASK: &str = r#"{"type":"Task","id":"{taskId}"}"#;

/// An Attachment goes on one record.
const ATTACH_TO_PERSON: &str = r#"{"target":{"type":"Person","id":"{personId}"}}"#;

/// The response `LINK_RISK` made, taken off again.
const REMOVE_RISK: &str = r#"{"type":"Risk","id":"{riskId}"}"#;

/// A link to the Sweep record of `risks`, for the groups a Risk responds to.
const LINK_RISK: &str = r#"{"responses":[{"type":"Risk","id":"{riskId}"}]}"#;

/// The Content language the Account has, and one it can be set to.
const LANGUAGES: &[&str] = &["en", "nl"];

/// The Framework the sweep adopts: no seeded Framework, the only one of its line, and one the seed's coverage gives
/// live Proposed coverage into. When the Account has adopted it already, no Framework can be adopted.
const ADOPTED_FRAMEWORK: FromRead = FromRead {
    name: "frameworkId",
    read: "/api/v1/frameworks",
    keep: &[("id", r#""iso-42001-2023""#), ("adopted", "false")],
    pointer: "/id",
    missing: Some("no adopted Framework"),
};

/// A Requirement of the adopted Framework that is not mandatory: a mandatory one always has a record, and always
/// applies, so it cannot be reset, materialised, or excluded.
const REQUIREMENT: FromRead = FromRead {
    name: "code",
    read: "/api/v1/frameworks/{frameworkId}/requirements",
    keep: &[("isMandatory", "false")],
    pointer: "/code",
    missing: None,
};

/// A live Proposed coverage of the adopted Framework, as the key the accept writes name it by.
const PROPOSAL: [FromRead; 3] = [
    FromRead {
        name: "requirementCode",
        read: "/api/v1/frameworks/{frameworkId}/proposed-coverage",
        keep: &[],
        pointer: "/requirementCode",
        missing: None,
    },
    FromRead {
        name: "artifactType",
        read: "/api/v1/frameworks/{frameworkId}/proposed-coverage",
        keep: &[],
        pointer: "/artifact/type",
        missing: None,
    },
    FromRead {
        name: "artifactId",
        read: "/api/v1/frameworks/{frameworkId}/proposed-coverage",
        keep: &[],
        pointer: "/artifact/id",
        missing: None,
    },
];

/// A mandatory Requirement of the adopted Framework: it always has a record and always applies, whatever the writes
/// of the Frameworks group did to the others, so a comment can be added to it and a record linked to it.
const MANDATORY_REQUIREMENT: FromRead = FromRead {
    keep: &[("isMandatory", "true")],
    ..REQUIREMENT
};

/// The adopted Framework's mandatory Requirement as the Requirement of a record.
const LINK_REQUIREMENT: Override = Override {
    id: "",
    body: Some(r#"{"requirements":[{"frameworkId":"{frameworkId}","code":"{code}"}]}"#),
    refs: &[("frameworkId", "frameworks")],
    from_read: &[MANDATORY_REQUIREMENT],
    ..NONE
};

/// The same on a Sweep record of its own: Compliance refuses to delete a record while a Requirement is linked to it,
/// so the group's own record, which the group's delete deletes, stays unlinked. The link goes with the Framework.
const LINK_REQUIREMENT_FRESH: Override = Override {
    fresh: true,
    ..LINK_REQUIREMENT
};

const OVERRIDES: &[Override] = &[
    Override {
        id: "accounts.switch",
        // `compliance accounts switch` is hand-written, and `accounts` is the `accounts` smoke Scenario's.
        skip: Some(
            "it moves the shared Personal token into another Account, which would break every later write",
        ),
        ..NONE
    },
    // settings: the Account has one of each, so a write reads the value first and sends it back after its checks.
    Override {
        id: "settings.update-assistant",
        // The Content language is free text in the description, and Compliance accepts only its supported languages.
        fixed: &[("contentLanguage", LANGUAGES)],
        ..NONE
    },
    Override {
        id: "settings.update-content-language",
        fixed: &[("contentLanguage", LANGUAGES)],
        ..NONE
    },
    // Compliance keeps the acts and Criticalities the write does not send, so the read lists more than it sent.
    Override {
        id: "settings.update-cadence",
        fixed: &[
            (
                "acts",
                &[r#"{"act":"PolicyReview","cadence":"EveryTwoYears"}"#],
            ),
            (
                "supplierReview",
                &[r#"{"criticality":"Critical","cadence":"EveryTwoYears"}"#],
            ),
        ],
        partial: &["acts", "supplierReview"],
        ..NONE
    },
    // It replaces the whole list: the write adds DE to the seeded NL, and the put-back sends the seeded list, which
    // removes DE again and leaves the adopted Sub-frameworks as they were. The one item the rules send through the
    // option would drop NL, which deletes the Requirement records of NL's Sub-framework, so `--body` sends both.
    Override {
        id: "settings.update-countries",
        body: Some(r#"{"countries":["DE","NL"]}"#),
        omit: &["countries"],
        ..NONE
    },
    // Compliance saves the Account's own text for the slot; `settings.restore-default-content` puts the product
    // default back, and sending the default as the Account's own text would not.
    Override {
        id: "settings.update-default-content",
        fixed: &[(
            "slots",
            &[r#"{"slot":"SupplierDescription","bodyHtml":"<p>E2E sweep default content</p>"}"#],
        )],
        partial: &["slots"],
        put_back_by: Some("settings.restore-default-content"),
        ..NONE
    },
    Override {
        id: "settings.restore-default-content",
        fixed: &[("slot", &["SupplierDescription"])],
        ..NONE
    },
    // scope: the Account has one, and every write to it changes its review: an update or a resume opens a review
    // Task of its own, which the cleanup of the Tasks the run made deletes, and the Scope is left with no next
    // review due. So none of them runs.
    Override {
        id: "scope.update",
        skip: Some(
            "it opens a review Task, and deleting the Task the run made leaves the Scope with no next review due, which the seed had",
        ),
        ..NONE
    },
    Override {
        id: "scope.delete",
        skip: Some(
            "it deletes the Scope the seed made, and writing a new one through `scope.update` is not that Scope",
        ),
        ..NONE
    },
    Override {
        id: "scope.opt-out",
        skip: Some(
            "it cancels the Scope's open review Task and adds to its review history, and nothing puts either back",
        ),
        ..NONE
    },
    Override {
        id: "scope.resume-review",
        skip: Some(
            "it adds to the Scope's review history, and the review Task it opens is not the one the seed made",
        ),
        ..NONE
    },
    // notifications: the Test-bed has none, so no read of the list changes. `mark-read` takes the id of a Comment.
    Override {
        id: "notifications.mark-read",
        refs: &[("commentId", "organisation-profile/comments")],
        no_read: Some(
            "the Account has no Notification, so the list reads the same before and after",
        ),
        ..NONE
    },
    Override {
        id: "notifications.mark-all-read",
        no_read: Some(
            "the Account has no Notification, so the list reads the same before and after, and marking one read cannot be undone",
        ),
        ..NONE
    },
    // legal-acceptance and license: the Test-bed's License is complimentary and has no Stripe key, so each of these
    // gives an answer the sweep checks as it is.
    Override {
        id: "legal-acceptance.accept",
        // Accepting is done in the browser.
        outcome: Some(Outcome {
            code: 3,
            status: None,
        }),
        ..NONE
    },
    Override {
        id: "license.subscribe",
        outcome: Some(Outcome {
            code: 1,
            status: Some(409),
        }),
        ..NONE
    },
    // A complimentary License needs no charge for a line, and an unknown line answers 422 instead.
    Override {
        id: "license.add-framework-line",
        fixed: &[("frameworkLine", &["iso-9001"])],
        outcome: Some(Outcome {
            code: 1,
            status: Some(409),
        }),
        ..NONE
    },
    // An Account with no subscription has nothing to cancel, so the answer is a success that changes nothing.
    Override {
        id: "license.cancel",
        outcome: Some(Outcome {
            code: 0,
            status: None,
        }),
        ..NONE
    },
    // Buying a pack charges money, so Compliance hands it to the browser. A pack the Test-bed does not sell answers
    // 422 before that, so none is sent. The body is empty, and sent with no body at all Compliance answers 415, so
    // it goes through `--body`.
    Override {
        id: "license.buy-allowance-pack",
        body: Some("{}"),
        omit: &["packUsd"],
        outcome: Some(Outcome {
            code: 3,
            status: None,
        }),
        ..NONE
    },
    // A disable saves at once and fills in a pack and a limit the seed leaves empty, which nothing can empty again,
    // so the sweep sends an enable: Compliance saves nothing and hands it to the browser.
    Override {
        id: "license.update-auto-topup",
        fixed: &[("enabled", &["true"])],
        omit: &["packUsd", "monthlyLimitUsd"],
        outcome: Some(Outcome {
            code: 3,
            status: None,
        }),
        ..NONE
    },
    // frameworks: the sweep adopts `ADOPTED_FRAMEWORK` as the group's Sweep record, and every write of the group
    // acts on it. Its Requirement writes act on `REQUIREMENT`, or on a mandatory one. The group runs before the others (`FIRST`), so the
    // writes that link a record to a Requirement find the Framework, and `license.drop-framework-line` and
    // `frameworks.unadopt` run after all of them (`LAST`).
    Override {
        id: "frameworks.adopt",
        from_read: &[ADOPTED_FRAMEWORK],
        made: Some(Made {
            collection: "/api/v1/frameworks",
            found_in: None,
        }),
        ..NONE
    },
    // Unadopting ends the Framework, so it acts on one of its own: it runs after the line is dropped, which ends
    // the Sweep record, so the same Framework can be adopted again.
    Override {
        id: "frameworks.unadopt",
        fresh: true,
        ends: Some("frameworkId"),
        ..NONE
    },
    // It unadopts every adopted edition of the Framework's line, which is the Sweep record's alone.
    Override {
        id: "license.drop-framework-line",
        refs: &[("frameworkId", "frameworks")],
        from_read: &[FromRead {
            name: "frameworkLine",
            read: "/api/v1/frameworks/{frameworkId}",
            keep: &[],
            pointer: "/frameworkLine",
            missing: None,
        }],
        ends: Some("frameworkId"),
        expects: Some(Expects {
            read: "/api/v1/frameworks/{frameworkId}",
            fields: &[("adopted", "false")],
        }),
        ..NONE
    },
    // The statement has no id of its own, and a slug is lowercase letters, numbers, and hyphens.
    Override {
        id: "frameworks.issue-statement",
        singleton: true,
        ..NONE
    },
    Override {
        id: "frameworks.update-statement",
        fixed: &[("slug", &["e2e-sweep-statement-1", "e2e-sweep-statement-2"])],
        ..NONE
    },
    Override {
        id: "frameworks.accept-proposed-coverage",
        body: Some(
            r#"{"proposal":{"requirementCode":"{requirementCode}","artifactType":"{artifactType}","artifactId":"{artifactId}"}}"#,
        ),
        from_read: &PROPOSAL,
        ..NONE
    },
    Override {
        id: "frameworks.accept-many-proposed-coverage",
        body: Some(
            r#"{"proposals":[{"requirementCode":"{requirementCode}","artifactType":"{artifactType}","artifactId":"{artifactId}"}]}"#,
        ),
        from_read: &PROPOSAL,
        ..NONE
    },
    // A comment is added to a Requirement that has a record, and the comments run before the writes that give a
    // Requirement one.
    Override {
        id: "frameworks.add-requirement-comment",
        from_read: &[MANDATORY_REQUIREMENT],
        ..NONE
    },
    Override {
        id: "frameworks.update-requirement-comment",
        from_read: &[MANDATORY_REQUIREMENT],
        ..NONE
    },
    Override {
        id: "frameworks.delete-requirement-comment",
        from_read: &[MANDATORY_REQUIREMENT],
        ..NONE
    },
    Override {
        id: "frameworks.materialise-requirement",
        from_read: &[REQUIREMENT],
        ..NONE
    },
    Override {
        id: "frameworks.opt-out-requirement",
        from_read: &[REQUIREMENT],
        ..NONE
    },
    Override {
        id: "frameworks.resume-requirement-review",
        from_read: &[REQUIREMENT],
        ..NONE
    },
    Override {
        id: "frameworks.reset-requirement",
        from_read: &[REQUIREMENT],
        ..NONE
    },
    Override {
        id: "frameworks.set-requirement-applicability",
        from_read: &[REQUIREMENT],
        ..NONE
    },
    Override {
        id: "frameworks.update-requirement",
        from_read: &[REQUIREMENT],
        ..NONE
    },
    // The writes that link a record to Requirements replace the whole list.
    Override {
        id: "risks.link-requirements",
        ..LINK_REQUIREMENT
    },
    Override {
        id: "business-issues.link-requirements",
        ..LINK_REQUIREMENT
    },
    Override {
        id: "interested-parties.link-requirements",
        ..LINK_REQUIREMENT
    },
    Override {
        id: "kpis.link-requirements",
        ..LINK_REQUIREMENT_FRESH
    },
    Override {
        id: "objectives.link-requirements",
        ..LINK_REQUIREMENT_FRESH
    },
    Override {
        id: "opportunities.link-requirements",
        ..LINK_REQUIREMENT
    },
    Override {
        id: "policies.link-requirements",
        ..LINK_REQUIREMENT
    },
    Override {
        id: "procedures.link-requirements",
        ..LINK_REQUIREMENT_FRESH
    },
    Override {
        id: "training-courses.link-requirements",
        ..LINK_REQUIREMENT
    },
    // risks
    // The residual scores are only accepted on a Mitigated Risk, an end date only on a Risk that has ended, and
    // `reviewOptOutReason` opts the Risk out of review, which `risks.opt-out` must do itself.
    Override {
        id: "risks.create",
        omit: &[
            "residualLikelihood",
            "residualImpact",
            "closedAt",
            "acceptedAt",
            "reviewOptOutReason",
        ],
        ..NONE
    },
    // assets
    Override {
        id: "assets.create",
        // Returning an Asset needs an Owner who is a Person, and reactivating one needs a Custodian.
        fixed: &[("ownerKind", &["Person"]), ("custodianKind", &["Person"])],
        refs: &[("ownerId", "people"), ("custodianId", "people")],
        omit: &["leftInventoryAt", "reviewOptOutReason"],
        ..NONE
    },
    Override {
        id: "assets.update",
        // Only an Information asset has a Retention period, and an update moves the Kind on from Information.
        omit: &[
            "leftInventoryAt",
            "retentionUnit",
            "retentionLength",
            "retentionStartingEvent",
        ],
        ..NONE
    },
    // business-issues
    Override {
        id: "business-issues.create",
        omit: &["archivedAt", "reviewOptOutReason"],
        ..NONE
    },
    Override {
        id: "business-issues.update",
        omit: &["archivedAt", "status"],
        ..NONE
    },
    Override {
        id: "business-issues.link-responses",
        body: Some(LINK_RISK),
        refs: &[("riskId", "risks")],
        ..NONE
    },
    Override {
        id: "business-issues.remove-response",
        body: Some(REMOVE_RISK),
        refs: &[("riskId", "risks")],
        omit: &["type", "id"],
        ..NONE
    },
    // changes
    Override {
        id: "changes.create",
        fixed: &[("externalLink", URLS)],
        ..NONE
    },
    Override {
        id: "changes.update",
        fixed: &[("externalLink", URLS)],
        omit: &["state"],
        ..NONE
    },
    // continuity-plans
    Override {
        id: "continuity-plans.create",
        omit: &["reviewOptOutReason"],
        ..NONE
    },
    // customers
    Override {
        id: "customers.create",
        body: Some(SECURITY_CONTACTS),
        ..NONE
    },
    // The Status moves through `customers.archive` and `customers.reactivate`.
    Override {
        id: "customers.update",
        omit: &["status"],
        ..NONE
    },
    // findings
    // The rejection date defaults to today; the rule's date is before the Finding's raised date.
    Override {
        id: "findings.reject",
        omit: &["rejectedAt"],
        ..NONE
    },
    Override {
        id: "findings.create",
        // A source type names a source record, which no schema-made value points at.
        omit: &["sourceType"],
        fixed: &[("externalLink", URLS)],
        ..NONE
    },
    Override {
        id: "findings.update",
        fixed: &[("externalLink", URLS)],
        omit: &["workflowState", "sourceType"],
        ..NONE
    },
    // incidents
    Override {
        id: "incidents.create",
        fixed: &[("externalLink", URLS)],
        ..NONE
    },
    Override {
        id: "incidents.update",
        fixed: &[("externalLink", URLS)],
        // The dates only mean something while the flag beside them stays on.
        omit: &["state", "isPersonalDataBreach", "isSignificantIncident"],
        ..NONE
    },
    // interested-parties
    Override {
        id: "interested-parties.create",
        omit: &["reviewOptOutReason"],
        ..NONE
    },
    Override {
        id: "interested-parties.update",
        omit: &["status"],
        ..NONE
    },
    Override {
        id: "interested-parties.link-responses",
        body: Some(LINK_RISK),
        refs: &[("riskId", "risks")],
        ..NONE
    },
    Override {
        id: "interested-parties.remove-response",
        body: Some(REMOVE_RISK),
        refs: &[("riskId", "risks")],
        omit: &["type", "id"],
        ..NONE
    },
    // kpis
    Override {
        id: "kpis.create",
        omit: &["archivedAt", "measurementOptOutReason"],
        ..NONE
    },
    Override {
        id: "kpis.update",
        omit: &["archivedAt"],
        ..NONE
    },
    // objectives
    Override {
        id: "objectives.create",
        omit: &["reviewOptOutReason"],
        ..NONE
    },
    Override {
        id: "objectives.update",
        omit: &["status"],
        ..NONE
    },
    // opportunities
    Override {
        id: "opportunities.create",
        omit: &["archivedAt", "reviewOptOutReason"],
        ..NONE
    },
    Override {
        id: "opportunities.update",
        omit: &["archivedAt", "status"],
        ..NONE
    },
    // policies
    Override {
        id: "policies.create",
        omit: &["reviewOptOutReason"],
        ..NONE
    },
    Override {
        id: "policies.update",
        // `TheseRoles` needs Roles, which no schema-made value points at.
        omit: &["archivedAt", "awarenessAudienceKind"],
        ..NONE
    },
    // procedures
    Override {
        id: "procedures.create",
        // Compliance refuses to delete a Procedure while a Task for it is open. `Everyone` opens an awareness Task
        // for each Person, where a Role with no holder opens none, and the review Task opens unless the Procedure
        // is created opted out (see `ACTION_ORDER`).
        fixed: &[("awarenessAudienceKind", &["TheseRoles"])],
        refs: &[("awarenessAudienceRoleIds", "roles")],
        ..NONE
    },
    Override {
        id: "procedures.update",
        omit: &["archivedAt", "awarenessAudienceKind"],
        ..NONE
    },
    // roles
    Override {
        id: "roles.add-holder",
        refs: &[("personId", "people")],
        ..NONE
    },
    Override {
        id: "roles.remove-holder",
        no_read: Some("the description has no read of a Role's holders"),
        refs: &[("personId", "people")],
        ..NONE
    },
    // people
    Override {
        id: "people.create",
        fixed: &[("email", EMAILS)],
        omit: &["endedAt"],
        ..NONE
    },
    Override {
        id: "people.update",
        omit: &["endedAt"],
        ..NONE
    },
    // The answer is the Person, and the new identity is the item of `identities` with the email sent.
    Override {
        id: "people.add-identity",
        fixed: &[("email", IDENTITY_EMAILS)],
        // A User id that no Person of the Account has.
        omit: &["linkedUserId"],
        made: Some(Made {
            collection: "/api/v1/people/{id}/identities",
            found_in: Some(("identities", "email")),
        }),
        ..NONE
    },
    // It folds the Person into the Sweep record of `people` and leaves that one.
    Override {
        id: "people.merge",
        refs: &[("intoPersonId", "people")],
        fresh: true,
        removes: true,
        ..NONE
    },
    // A new Person gets the Training assignments of the courses for everyone, and Compliance refuses to delete a
    // Person while one is open.
    Override {
        id: "people.delete",
        fresh: true,
        clears_tasks: true,
        ..NONE
    },
    // The departure date cannot be before the start date, which the rule's dates put at March 2026.
    Override {
        id: "people.depart",
        fixed: &[("endedAt", &["2099-12-31"])],
        ..NONE
    },
    // invitations: no email is sent (the Launcher turns it off), and the description has no read of Invitations.
    Override {
        id: "invitations.send",
        refs: &[("personIds", "people")],
        no_read: Some("the description has no read of Invitations"),
        ..NONE
    },
    // events
    // A Framework needs adopting, and attendees are only for a Meeting.
    Override {
        id: "events.create",
        omit: &["frameworkId", "attendees"],
        ..NONE
    },
    Override {
        id: "events.update",
        omit: &["frameworkId", "attendees"],
        ..NONE
    },
    // The Finding and the Vulnerability are Sweep records of their own groups.
    Override {
        id: "events.create-finding",
        fixed: &[("externalLink", URLS)],
        made: Some(Made {
            collection: "/api/v1/findings",
            found_in: None,
        }),
        ..NONE
    },
    Override {
        id: "events.create-vulnerability",
        fixed: &[("externalLink", URLS)],
        made: Some(Made {
            collection: "/api/v1/vulnerabilities",
            found_in: None,
        }),
        ..NONE
    },
    // A Finding or Vulnerability of the Event blocks the Event's delete, so the delete acts on an Event of its own.
    Override {
        id: "events.delete",
        fresh: true,
        ..NONE
    },
    // `id` is the Risk, as the body field and as the path placeholder of the target alike.
    Override {
        id: "events.add-target",
        body: Some(TARGET_RISK),
        refs: &[("riskId", "risks")],
        omit: &["type", "id"],
        ..NONE
    },
    Override {
        id: "events.remove-target",
        fixed: &[("type", &["Risk"])],
        refs: &[("targetId", "risks")],
        ..NONE
    },
    // evidence
    // Evidence is attached only to a Task.
    Override {
        id: "evidence.link",
        body: Some(TARGET_TASK),
        refs: &[("taskId", "tasks")],
        omit: &["type", "id"],
        ..NONE
    },
    Override {
        id: "evidence.detach",
        fixed: &[("type", &["Task"])],
        refs: &[("targetId", "tasks")],
        ..NONE
    },
    // attachments: the file goes through `--file`, since a web address makes Compliance fetch it.
    Override {
        id: "attachments.create",
        omit: &["webAddress"],
        body: Some(ATTACH_TO_PERSON),
        refs: &[("personId", "people")],
        ..NONE
    },
    // training-courses
    Override {
        id: "training-courses.create",
        fixed: &[("contentUrl", URLS)],
        ..NONE
    },
    Override {
        id: "training-courses.update",
        fixed: &[("contentUrl", URLS)],
        ..NONE
    },
    // A course of the library, which the Test-bed's Course library holds. Adding a course twice answers the same
    // course, so a second run changes nothing the list shows.
    Override {
        id: "training-courses.add-from-library",
        fixed: &[("slug", &["phishing-messages"])],
        no_read: Some("adding a library course again changes nothing a read shows"),
        ..NONE
    },
    // A due date is today or later.
    Override {
        id: "training-courses.assign",
        fixed: &[("dueAt", &["2099-12-31"])],
        refs: &[("personIds", "people")],
        ..NONE
    },
    Override {
        id: "training-courses.assign-all",
        fixed: &[("dueAt", &["2099-12-31"])],
        ..NONE
    },
    // uploads: the link takes the file later; nothing reads an Upload link back.
    Override {
        id: "uploads.create",
        fixed: &[("target", &["evidence"]), ("fileName", &["e2e-sweep.txt"])],
        no_read: Some("the description has no read of an Upload link"),
        ..NONE
    },
    // schedules: a Framework pin needs an adopted Framework, and Kind and pin never change after create.
    Override {
        id: "schedules.create",
        omit: &["frameworkId"],
        ..NONE
    },
    Override {
        id: "schedules.update",
        omit: &["frameworkId", "kind"],
        ..NONE
    },
    // suppliers
    Override {
        id: "suppliers.create",
        fixed: &[("website", WEBSITES), ("country", COUNTRIES)],
        omit: &["terminatedAt", "reviewOptOutReason"],
        ..NONE
    },
    Override {
        id: "suppliers.update",
        fixed: &[("website", WEBSITES), ("country", COUNTRIES)],
        omit: &["terminatedAt", "status"],
        ..NONE
    },
    // tasks
    Override {
        id: "tasks.update",
        // The State moves through `tasks.start`, `tasks.complete`, and `tasks.cancel`.
        omit: &["state"],
        ..NONE
    },
    // The rule's date is the one `tasks.complete` recorded.
    Override {
        id: "tasks.correct-recorded-date",
        fixed: &[("recordedAt", &["2026-03-14"])],
        ..NONE
    },
    // Cancelling ends the Task, which the rest of the group's writes need open or done.
    Override {
        id: "tasks.cancel",
        fresh: true,
        ..NONE
    },
    // It acts on an open Asset review task, which a Test-bed with no Asset review does not have: a body that is
    // well formed but sent to a Task that is no open Asset review answers 409.
    Override {
        id: "tasks.change-custodian",
        fixed: &[("custodianKind", &["Person"])],
        refs: &[("assetId", "assets"), ("custodianId", "people")],
        outcome: Some(Outcome {
            code: 1,
            status: Some(409),
        }),
        ..NONE
    },
    // vulnerabilities
    Override {
        id: "vulnerabilities.create",
        fixed: &[("externalLink", URLS)],
        ..NONE
    },
    Override {
        id: "vulnerabilities.update",
        fixed: &[("externalLink", URLS)],
        omit: &["workflowState"],
        ..NONE
    },
    // `accept-risk` completes the Remediation task, so this needs a Vulnerability of its own.
    Override {
        id: "vulnerabilities.mark-false-positive",
        fresh: true,
        ..NONE
    },
    Override {
        id: "vulnerabilities.accept-risk",
        refs: &[("riskId", "risks")],
        ..NONE
    },
    // A treatment action is due today or later, and the rule's date is in the past.
    Override {
        id: "risks.add-treatment-action",
        fixed: &[("dueAt", &["2099-12-31"])],
        ..NONE
    },
    Override {
        id: "risks.update",
        omit: &[
            "residualLikelihood",
            "residualImpact",
            "closedAt",
            "acceptedAt",
        ],
        ..NONE
    },
    // conversations: a Conversation has no delete, so the one `conversations.create` makes stays on the Test-bed.
    // The reply to a message comes in the background and is not waited for.
    Override {
        id: "conversations.send-message",
        singleton: true,
        ..NONE
    },
    Override {
        id: "conversations.stop",
        no_read: Some(
            "a stopped Turn leaves the Conversation as an idle one reads, and no read tells the two apart",
        ),
        ..NONE
    },
    // frameworks: a gap analysis opens a shared Conversation, which the Framework's read does not show.
    Override {
        id: "frameworks.start-gap-analysis",
        fixed: &[("startOver", &["true"])],
        no_read: Some("the Framework's read does not show the Conversation the gap analysis opens"),
        ..NONE
    },
    // checks: a run works in the background and, when it is done, replaces the results of its own Check in the inbox.
    // The seed has results of every Check, so any run removes a seeded result, and nothing puts it back.
    Override {
        id: "checks.start",
        skip: Some(
            "a run replaces the seeded results of its Check in the inbox, and the results a run makes exist only once it is done, which the sweep does not wait for",
        ),
        ..NONE
    },
    // Accepting changes no record and leaves the result in the inbox: it only opens a Conversation, which has no
    // delete. It takes a seeded result, because a result a run makes exists only once the run is done.
    Override {
        id: "check-results.accept",
        from_read: &[FromRead {
            name: "id",
            read: "/api/v1/check-results",
            keep: &[("check", r#""UncoveredRisk""#)],
            pointer: "/id",
            missing: None,
        }],
        no_read: Some(
            "accepting leaves the result in the inbox and opens a Conversation, which the inbox does not show",
        ),
        ..NONE
    },
    Override {
        id: "check-results.decline",
        skip: Some(
            "it removes a Check result the seed made, and nothing puts it back; a result a run makes exists only once the run is done, which the sweep does not wait for",
        ),
        ..NONE
    },
    // import-sessions: the file is not an Import export, so Compliance's prepare of it fails within a moment and the
    // session is never `Prepared`. The sweep does not wait for that moment: each write that depends on it comes
    // after commands that take longer. A run starts only on a `Prepared` session (409), and a page is marked
    // only on one that has pages (404). A retry of the failed prepare starts it again, and what a read shows of it
    // is the same as before.
    Override {
        id: "import-sessions.start",
        fixed: &[("mode", &["full"])],
        outcome: Some(Outcome {
            code: 1,
            status: Some(409),
        }),
        ..NONE
    },
    Override {
        id: "import-sessions.mark-page",
        fixed: &[("pageId", &["e2e-sweep-page"])],
        outcome: Some(Outcome {
            code: 1,
            status: Some(404),
        }),
        ..NONE
    },
    Override {
        id: "import-sessions.retry",
        no_read: Some(
            "the prepare fails again at once, so the session reads as it did before the retry",
        ),
        ..NONE
    },
];

/// Groups whose writes run before every other group's. Another group's write can make a Sweep record of one of
/// them (a Procedure for a Role), and if the group's own create ran later it would make a second record with the
/// same name, which Compliance refuses.
/// The Frameworks group runs first too: the writes that link a record to a Requirement need the adopted Framework.
const FIRST: &[&str] = &["roles", "frameworks"];

/// Whether the writes of `group` run before the other groups' writes.
pub fn runs_first(group: &str) -> bool {
    FIRST.contains(&group)
}

/// Writes that run after every other write, in this order: they end the adopted Framework, which the writes of
/// other groups need. The first one ends the Framework the sweep adopted, so the second acts on one of its own.
const LAST: &[&str] = &["license.drop-framework-line", "frameworks.unadopt"];

/// Where `operation_id` comes among the writes that run last, or `None` when it does not run last.
pub fn last_rank(operation_id: &str) -> Option<usize> {
    LAST.iter().position(|id| *id == operation_id)
}

/// How a Sweep record of a collection without a delete ends, so the clean-up can end one a run left: the id of the
/// write that takes the record's id as its first path value.
const ENDED_BY: &[(&str, &str)] = &[("/api/v1/frameworks", "frameworks.unadopt")];

/// The id of the write that ends a Sweep record of `collection`, when the collection has no delete.
pub fn ended_by(collection: &str) -> Option<&'static str> {
    ENDED_BY
        .iter()
        .find(|(name, _)| *name == collection)
        .map(|(_, id)| *id)
}

/// The actions of a group that must run in this order, because each leaves the record in the state the next one
/// needs: an action that ends the record comes after the ones that need it live, and the one that undoes it after
/// that. Actions a group does not list run after these, by path.
const ACTION_ORDER: &[(&str, &[&str])] = &[
    // The Requirement is not reviewed when the Framework is adopted: it is materialised, opted out of review and
    // back, and reset, and then marked applicable and updated, which need no record before them.
    (
        "frameworks",
        &[
            "accept-proposed-coverage",
            "accept-many-proposed-coverage",
            "materialise-requirement",
            "opt-out-requirement",
            "resume-requirement-review",
            "reset-requirement",
            "set-requirement-applicability",
            "update-requirement",
        ],
    ),
    (
        "assets",
        &[
            "opt-out",
            "resume-review",
            "turn-off-access-review",
            "turn-on-access-review",
            "return-to-owner",
            "reactivate",
            "retire",
        ],
    ),
    (
        "business-issues",
        &[
            "link-responses",
            "remove-response",
            "opt-out",
            "resume-review",
            "archive",
            "reactivate",
        ],
    ),
    (
        "interested-parties",
        &[
            "link-responses",
            "remove-response",
            "opt-out",
            "resume-review",
            "archive",
            "reactivate",
        ],
    ),
    // A retry comes after the run is refused, and the page is marked after it: the commands between them give the
    // failed prepare time to finish before the delete.
    ("import-sessions", &["start", "retry", "mark-page"]),
    (
        "kpis",
        &["opt-out", "resume-review", "archive", "unarchive"],
    ),
    (
        "opportunities",
        &["opt-out", "resume-review", "archive", "reactivate"],
    ),
    (
        "people",
        &[
            "add-identity",
            "set-primary-identity",
            "depart",
            "reactivate",
            "merge",
        ],
    ),
    (
        "policies",
        &["opt-out", "resume-review", "archive", "unarchive"],
    ),
    // `assign-all` skips the People who have an assignment of the course, and then `assign` has no one to assign.
    ("training-courses", &["assign", "assign-all"]),
    // Created opted out of review, and left opted out, with no review Task open: Compliance refuses to delete a
    // Procedure while one is, and `unarchive` opens one while the review is on.
    (
        "procedures",
        &["resume-review", "archive", "unarchive", "opt-out"],
    ),
    (
        "suppliers",
        &["opt-out", "resume-review", "terminate", "reactivate"],
    ),
    (
        "tasks",
        &["start", "save-result", "complete", "correct-recorded-date"],
    ),
];

/// Where the `action` of a write in `group` comes in its group's actions; an unlisted action comes last.
pub fn action_rank(group: &str, action: &str) -> usize {
    ACTION_ORDER
        .iter()
        .find(|(name, _)| *name == group)
        .and_then(|(_, actions)| actions.iter().position(|listed| *listed == action))
        .unwrap_or(usize::MAX)
}

/// The entry for the operation `id`, or one that asks for nothing.
pub fn entry(id: &str) -> &'static Override {
    OVERRIDES
        .iter()
        .find(|entry| entry.id == id)
        .unwrap_or(&NONE)
}
