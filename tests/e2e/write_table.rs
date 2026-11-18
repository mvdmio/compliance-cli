pub struct Outcome {
    pub code: i32,
    pub status: Option<u16>,
}

pub struct Made {
    pub collection: &'static str,
    pub found_in: Option<(&'static str, &'static str)>,
}

pub struct FromRead {
    pub name: &'static str,
    pub read: &'static str,
    pub keep: &'static [(&'static str, &'static str)],
    pub pointer: &'static str,
    pub missing: Option<&'static str>,
}

pub struct Expects {
    pub read: &'static str,
    pub fields: &'static [(&'static str, &'static str)],
}

pub struct Override {
    pub id: &'static str,
    pub omit: &'static [&'static str],
    pub fixed: &'static [(&'static str, &'static [&'static str])],
    pub refs: &'static [(&'static str, &'static str)],
    pub body: Option<&'static str>,
    pub fresh: bool,
    pub made: Option<Made>,
    pub removes: bool,
    pub clears_tasks: bool,
    pub skip: Option<&'static str>,
    pub no_read: Option<&'static str>,
    pub reason: Option<&'static str>,
    pub outcome: Option<Outcome>,
    pub partial: &'static [&'static str],
    pub put_back_by: Option<&'static str>,
    pub from_read: &'static [FromRead],
    pub expects: Option<Expects>,
    pub ends: Option<&'static str>,
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
    reason: None,
    outcome: None,
    partial: &[],
    put_back_by: None,
    from_read: &[],
    expects: None,
    ends: None,
    singleton: false,
};

const URLS: &[&str] = &[
    "https://example.com/e2e-sweep-1",
    "https://example.com/e2e-sweep-2",
];

const EMAILS: &[&str] = &["e2e-sweep-{n}@example.com"];

const COUNTRIES: &[&str] = &["NL", "DE"];

const WEBSITES: &[&str] = &[
    "https://e2e-sweep-1.example.com",
    "https://e2e-sweep-2.example.com",
];

const SECURITY_CONTACTS: &str =
    r#"{"securityContacts":[{"name":"E2E sweep contact","email":"e2e-sweep@example.com"}]}"#;

const IDENTITY_EMAILS: &[&str] = &["e2e-sweep-identity-{n}@example.com"];

const TARGET_RISK: &str = r#"{"type":"Risk","id":"{riskId}"}"#;
const TARGET_TASK: &str = r#"{"type":"Task","id":"{taskId}"}"#;

const ATTACH_TO_PERSON: &str = r#"{"target":{"type":"Person","id":"{personId}"}}"#;

const REMOVE_RISK: &str = r#"{"type":"Risk","id":"{riskId}"}"#;

const LINK_RISK: &str = r#"{"responses":[{"type":"Risk","id":"{riskId}"}]}"#;

const LANGUAGES: &[&str] = &["en", "nl"];

const ADOPTED_FRAMEWORK: FromRead = FromRead {
    name: "frameworkId",
    read: "/api/v1/frameworks",
    keep: &[("id", r#""iso-42001-2023""#), ("adopted", "false")],
    pointer: "/id",
    missing: Some("no adopted Framework"),
};

const REQUIREMENT: FromRead = FromRead {
    name: "code",
    read: "/api/v1/frameworks/{frameworkId}/requirements",
    keep: &[("isMandatory", "false")],
    pointer: "/code",
    missing: None,
};

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

const MANDATORY_REQUIREMENT: FromRead = FromRead {
    keep: &[("isMandatory", "true")],
    ..REQUIREMENT
};

const LINK_REQUIREMENT: Override = Override {
    id: "",
    body: Some(r#"{"requirements":[{"frameworkId":"{frameworkId}","code":"{code}"}]}"#),
    refs: &[("frameworkId", "frameworks")],
    from_read: &[MANDATORY_REQUIREMENT],
    reason: Some("the writes that link a record to Requirements replace the whole list"),
    ..NONE
};

const LINK_REQUIREMENT_FRESH: Override = Override {
    fresh: true,
    ..LINK_REQUIREMENT
};

const OVERRIDES: &[Override] = &[
    Override {
        id: "accounts.switch",
        skip: Some(
            "it moves the shared Personal token into another Account, which would break every later write",
        ),
        ..NONE
    },
    Override {
        id: "settings.update-assistant",
        fixed: &[("contentLanguage", LANGUAGES)],
        reason: Some(
            "the Content language is free text in the description and Compliance accepts only its supported languages, and the Account has one setting, which the write reads first and sends back after its checks",
        ),
        ..NONE
    },
    Override {
        id: "settings.update-content-language",
        fixed: &[("contentLanguage", LANGUAGES)],
        reason: Some("the Account has one setting, read first and sent back after the checks"),
        ..NONE
    },
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
        reason: Some("Compliance keeps the acts and Criticalities not sent, so the read has more"),
        ..NONE
    },
    Override {
        id: "settings.update-countries",
        body: Some(r#"{"countries":["DE","NL"]}"#),
        omit: &["countries"],
        reason: Some(
            "it replaces the whole list, and the one item the rules send would drop NL, which deletes the Requirement records of NL's Sub-framework, so `--body` sends DE and NL, and the put-back of the seeded list removes DE again and leaves the adopted Sub-frameworks as they were",
        ),
        ..NONE
    },
    Override {
        id: "settings.update-default-content",
        fixed: &[(
            "slots",
            &[r#"{"slot":"SupplierDescription","bodyHtml":"<p>E2E sweep default content</p>"}"#],
        )],
        partial: &["slots"],
        put_back_by: Some("settings.restore-default-content"),
        reason: Some("sending the default as the slot's text saves it as the Account's own"),
        ..NONE
    },
    Override {
        id: "settings.restore-default-content",
        fixed: &[("slot", &["SupplierDescription"])],
        ..NONE
    },
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
    Override {
        id: "legal-acceptance.accept",
        outcome: Some(Outcome {
            code: 3,
            status: None,
        }),
        reason: Some("accepting is done in the browser, so the answer is a Browser handoff"),
        ..NONE
    },
    Override {
        id: "license.subscribe",
        outcome: Some(Outcome {
            code: 1,
            status: Some(409),
        }),
        reason: Some("the Test-bed's License is complimentary and has no Stripe key"),
        ..NONE
    },
    Override {
        id: "license.add-framework-line",
        fixed: &[("frameworkLine", &["iso-9001"])],
        outcome: Some(Outcome {
            code: 1,
            status: Some(409),
        }),
        reason: Some("a complimentary License needs no charge, and an unknown line answers 422"),
        ..NONE
    },
    Override {
        id: "license.cancel",
        outcome: Some(Outcome {
            code: 0,
            status: None,
        }),
        reason: Some("with no subscription there is nothing to cancel, so nothing changes"),
        ..NONE
    },
    Override {
        id: "license.buy-allowance-pack",
        body: Some("{}"),
        omit: &["packUsd"],
        outcome: Some(Outcome {
            code: 3,
            status: None,
        }),
        reason: Some(
            "buying a pack charges money, so Compliance hands it to the browser, and a pack the Test-bed does not sell answers 422 before that, so none is sent, and the body is empty and goes through `--body` because sent with no body at all Compliance answers 415",
        ),
        ..NONE
    },
    Override {
        id: "license.update-auto-topup",
        fixed: &[("enabled", &["true"])],
        omit: &["packUsd", "monthlyLimitUsd"],
        outcome: Some(Outcome {
            code: 3,
            status: None,
        }),
        reason: Some(
            "a disable saves at once and fills in a pack and a limit the seed leaves empty, which nothing can empty again, so the sweep sends an enable, and Compliance saves nothing and hands it to the browser",
        ),
        ..NONE
    },
    Override {
        id: "frameworks.adopt",
        from_read: &[ADOPTED_FRAMEWORK],
        made: Some(Made {
            collection: "/api/v1/frameworks",
            found_in: None,
        }),
        ..NONE
    },
    Override {
        id: "frameworks.unadopt",
        reason: Some("unadopting ends the Framework, so it acts on one of its own"),
        fresh: true,
        ends: Some("frameworkId"),
        ..NONE
    },
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
        reason: Some("it unadopts every edition of the Framework's line, the record's alone"),
        ..NONE
    },
    Override {
        id: "frameworks.issue-statement",
        singleton: true,
        reason: Some("the statement has no id of its own"),
        ..NONE
    },
    Override {
        id: "frameworks.update-statement",
        fixed: &[("slug", &["e2e-sweep-statement-1", "e2e-sweep-statement-2"])],
        reason: Some("a slug is lowercase letters, numbers, and hyphens"),
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
    Override {
        id: "frameworks.add-requirement-comment",
        from_read: &[MANDATORY_REQUIREMENT],
        reason: Some("a comment goes on a Requirement with a record, so a mandatory one"),
        ..NONE
    },
    Override {
        id: "frameworks.update-requirement-comment",
        from_read: &[MANDATORY_REQUIREMENT],
        reason: Some("a comment goes on a Requirement with a record, so a mandatory one"),
        ..NONE
    },
    Override {
        id: "frameworks.delete-requirement-comment",
        from_read: &[MANDATORY_REQUIREMENT],
        reason: Some("a comment goes on a Requirement with a record, so a mandatory one"),
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
    Override {
        id: "risks.create",
        omit: &[
            "residualLikelihood",
            "residualImpact",
            "closedAt",
            "acceptedAt",
            "reviewOptOutReason",
        ],
        reason: Some(
            "the residual scores are only accepted on a Mitigated Risk, an end date only on a Risk that has ended, and `reviewOptOutReason` opts the Risk out of review, which `risks.opt-out` must do itself",
        ),
        ..NONE
    },
    Override {
        id: "assets.create",
        fixed: &[("ownerKind", &["Person"]), ("custodianKind", &["Person"])],
        refs: &[("ownerId", "people"), ("custodianId", "people")],
        omit: &["leftInventoryAt", "reviewOptOutReason"],
        reason: Some("returning an Asset needs a Person as Owner, reactivating one a Custodian"),
        ..NONE
    },
    Override {
        id: "assets.update",
        omit: &[
            "leftInventoryAt",
            "retentionUnit",
            "retentionLength",
            "retentionStartingEvent",
        ],
        reason: Some("an update moves the Kind off Information, the only Kind with Retention"),
        ..NONE
    },
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
    Override {
        id: "continuity-plans.create",
        omit: &["reviewOptOutReason"],
        ..NONE
    },
    Override {
        id: "customers.create",
        body: Some(SECURITY_CONTACTS),
        ..NONE
    },
    Override {
        id: "customers.update",
        omit: &["status"],
        reason: Some("the Status moves through `customers.archive` and `customers.reactivate`"),
        ..NONE
    },
    Override {
        id: "findings.reject",
        omit: &["rejectedAt"],
        reason: Some("the rule's date is before the raised date, and the default is today"),
        ..NONE
    },
    Override {
        id: "findings.create",
        omit: &["sourceType"],
        fixed: &[("externalLink", URLS)],
        reason: Some("a source type names a source record, which no schema-made value points at"),
        ..NONE
    },
    Override {
        id: "findings.update",
        fixed: &[("externalLink", URLS)],
        omit: &["workflowState", "sourceType"],
        ..NONE
    },
    Override {
        id: "incidents.create",
        fixed: &[("externalLink", URLS)],
        ..NONE
    },
    Override {
        id: "incidents.update",
        fixed: &[("externalLink", URLS)],
        omit: &["state", "isPersonalDataBreach", "isSignificantIncident"],
        reason: Some("the dates only mean something while the flag beside them stays on"),
        ..NONE
    },
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
    Override {
        id: "policies.create",
        omit: &["reviewOptOutReason"],
        ..NONE
    },
    Override {
        id: "policies.update",
        omit: &["archivedAt", "awarenessAudienceKind"],
        reason: Some("`TheseRoles` needs Roles, which no schema-made value points at"),
        ..NONE
    },
    Override {
        id: "procedures.create",
        fixed: &[("awarenessAudienceKind", &["TheseRoles"])],
        refs: &[("awarenessAudienceRoleIds", "roles")],
        reason: Some(
            "Compliance refuses to delete a Procedure while a Task for it is open, and `Everyone` opens an awareness Task for each Person, where a Role with no holder opens none, and the review Task opens unless the Procedure is created opted out",
        ),
        ..NONE
    },
    Override {
        id: "procedures.update",
        omit: &["archivedAt", "awarenessAudienceKind"],
        ..NONE
    },
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
    Override {
        id: "people.add-identity",
        fixed: &[("email", IDENTITY_EMAILS)],
        omit: &["linkedUserId"],
        made: Some(Made {
            collection: "/api/v1/people/{id}/identities",
            found_in: Some(("identities", "email")),
        }),
        reason: Some(
            "the answer is the Person, and the new identity is the item of `identities` with the email sent, and a `linkedUserId` made from the schema is a User id that no Person of the Account has",
        ),
        ..NONE
    },
    Override {
        id: "people.merge",
        refs: &[("intoPersonId", "people")],
        fresh: true,
        removes: true,
        reason: Some("it folds the Person into the Sweep record of `people` and leaves that one"),
        ..NONE
    },
    Override {
        id: "people.delete",
        fresh: true,
        clears_tasks: true,
        reason: Some("open Training assignments block a Person's delete, and a new one has them"),
        ..NONE
    },
    Override {
        id: "people.depart",
        fixed: &[("endedAt", &["2099-12-31"])],
        reason: Some("the departure date cannot be before the start date, March 2026"),
        ..NONE
    },
    Override {
        id: "invitations.send",
        refs: &[("personIds", "people")],
        no_read: Some("the description has no read of Invitations"),
        reason: Some("no email is sent, because the Launcher turns it off"),
        ..NONE
    },
    Override {
        id: "events.create",
        omit: &["frameworkId", "attendees"],
        reason: Some("a Framework needs adopting, and attendees are only for a Meeting"),
        ..NONE
    },
    Override {
        id: "events.update",
        omit: &["frameworkId", "attendees"],
        reason: Some("a Framework needs adopting, and attendees are only for a Meeting"),
        ..NONE
    },
    Override {
        id: "events.create-finding",
        fixed: &[("externalLink", URLS)],
        made: Some(Made {
            collection: "/api/v1/findings",
            found_in: None,
        }),
        reason: Some("the Finding and the Vulnerability are Sweep records of their own groups"),
        ..NONE
    },
    Override {
        id: "events.create-vulnerability",
        fixed: &[("externalLink", URLS)],
        made: Some(Made {
            collection: "/api/v1/vulnerabilities",
            found_in: None,
        }),
        reason: Some("the Finding and the Vulnerability are Sweep records of their own groups"),
        ..NONE
    },
    Override {
        id: "events.delete",
        fresh: true,
        reason: Some("a Finding or Vulnerability blocks the Event's delete, so it gets its own"),
        ..NONE
    },
    Override {
        id: "events.add-target",
        body: Some(TARGET_RISK),
        refs: &[("riskId", "risks")],
        omit: &["type", "id"],
        reason: Some("`id` is the Risk, as the body field and the path placeholder alike"),
        ..NONE
    },
    Override {
        id: "events.remove-target",
        fixed: &[("type", &["Risk"])],
        refs: &[("targetId", "risks")],
        ..NONE
    },
    Override {
        id: "evidence.link",
        body: Some(TARGET_TASK),
        refs: &[("taskId", "tasks")],
        omit: &["type", "id"],
        reason: Some("Evidence is attached only to a Task"),
        ..NONE
    },
    Override {
        id: "evidence.detach",
        fixed: &[("type", &["Task"])],
        refs: &[("targetId", "tasks")],
        reason: Some("Evidence is attached only to a Task"),
        ..NONE
    },
    Override {
        id: "attachments.create",
        omit: &["webAddress"],
        body: Some(ATTACH_TO_PERSON),
        refs: &[("personId", "people")],
        reason: Some("the file goes through `--file`, as a web address makes Compliance fetch it"),
        ..NONE
    },
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
    Override {
        id: "training-courses.add-from-library",
        fixed: &[("slug", &["phishing-messages"])],
        no_read: Some("adding a library course again changes nothing a read shows"),
        reason: Some("the slug is a course the Test-bed's Course library holds"),
        ..NONE
    },
    Override {
        id: "training-courses.assign",
        fixed: &[("dueAt", &["2099-12-31"])],
        refs: &[("personIds", "people")],
        reason: Some("a due date is today or later"),
        ..NONE
    },
    Override {
        id: "training-courses.assign-all",
        fixed: &[("dueAt", &["2099-12-31"])],
        reason: Some("a due date is today or later"),
        ..NONE
    },
    Override {
        id: "uploads.create",
        fixed: &[("target", &["evidence"]), ("fileName", &["e2e-sweep.txt"])],
        no_read: Some("the description has no read of an Upload link"),
        reason: Some("the link takes the file later"),
        ..NONE
    },
    Override {
        id: "schedules.create",
        omit: &["frameworkId"],
        reason: Some("a Framework pin needs an adopted Framework"),
        ..NONE
    },
    Override {
        id: "schedules.update",
        omit: &["frameworkId", "kind"],
        reason: Some("a pin needs an adopted Framework; Kind and pin never change after create"),
        ..NONE
    },
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
    Override {
        id: "tasks.update",
        omit: &["state"],
        reason: Some("the State moves through `tasks.start`, `tasks.complete`, `tasks.cancel`"),
        ..NONE
    },
    Override {
        id: "tasks.correct-recorded-date",
        fixed: &[("recordedAt", &["2026-03-14"])],
        reason: Some("the rule's date is the one `tasks.complete` recorded"),
        ..NONE
    },
    Override {
        id: "tasks.cancel",
        fresh: true,
        reason: Some("cancelling ends the Task, which the group's other writes need open or done"),
        ..NONE
    },
    Override {
        id: "tasks.change-custodian",
        fixed: &[("custodianKind", &["Person"])],
        refs: &[("assetId", "assets"), ("custodianId", "people")],
        outcome: Some(Outcome {
            code: 1,
            status: Some(409),
        }),
        reason: Some("no open Asset review Task exists, so a well formed body answers 409"),
        ..NONE
    },
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
    Override {
        id: "vulnerabilities.mark-false-positive",
        fresh: true,
        reason: Some("`accept-risk` completes the Remediation task, so this needs its own record"),
        ..NONE
    },
    Override {
        id: "vulnerabilities.accept-risk",
        refs: &[("riskId", "risks")],
        ..NONE
    },
    Override {
        id: "risks.add-treatment-action",
        fixed: &[("dueAt", &["2099-12-31"])],
        reason: Some("a treatment action is due today or later, and the rule's date is past"),
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
    Override {
        id: "conversations.send-message",
        singleton: true,
        reason: Some("a Conversation has no delete, so the one `conversations.create` makes stays"),
        ..NONE
    },
    Override {
        id: "conversations.stop",
        no_read: Some(
            "a stopped Turn leaves the Conversation as an idle one reads, and no read tells the two apart",
        ),
        reason: Some("a Conversation has no delete, so the one `conversations.create` makes stays"),
        ..NONE
    },
    Override {
        id: "frameworks.start-gap-analysis",
        fixed: &[("startOver", &["true"])],
        no_read: Some("the Framework's read does not show the Conversation the gap analysis opens"),
        ..NONE
    },
    Override {
        id: "checks.start",
        skip: Some(
            "a run replaces the seeded results of its Check in the inbox, and the results a run makes exist only once it is done, which the sweep does not wait for",
        ),
        ..NONE
    },
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
        reason: Some("a result a run makes exists only once done, so it takes a seeded one"),
        ..NONE
    },
    Override {
        id: "check-results.decline",
        skip: Some(
            "it removes a Check result the seed made, and nothing puts it back; a result a run makes exists only once the run is done, which the sweep does not wait for",
        ),
        ..NONE
    },
    Override {
        id: "import-sessions.start",
        reason: Some(
            "the file is not an Import export, and a run starts only on a Prepared session",
        ),
        fixed: &[("mode", &["full"])],
        outcome: Some(Outcome {
            code: 1,
            status: Some(409),
        }),
        ..NONE
    },
    Override {
        id: "import-sessions.mark-page",
        reason: Some(
            "the file is not an Import export, and a page is marked only on a session that has pages",
        ),
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

const FIRST: &[&str] = &["roles", "frameworks"];

pub fn runs_first(group: &str) -> bool {
    FIRST.contains(&group)
}

const LAST: &[&str] = &["license.drop-framework-line", "frameworks.unadopt"];

pub fn last_rank(operation_id: &str) -> Option<usize> {
    LAST.iter().position(|id| *id == operation_id)
}

const ENDED_BY: &[(&str, &str)] = &[("/api/v1/frameworks", "frameworks.unadopt")];

pub fn ended_by(collection: &str) -> Option<&'static str> {
    ENDED_BY
        .iter()
        .find(|(name, _)| *name == collection)
        .map(|(_, id)| *id)
}

const ACTION_ORDER: &[(&str, &[&str])] = &[
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
    ("training-courses", &["assign", "assign-all"]),
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

pub fn action_rank(group: &str, action: &str) -> usize {
    ACTION_ORDER
        .iter()
        .find(|(name, _)| *name == group)
        .and_then(|(_, actions)| actions.iter().position(|listed| *listed == action))
        .unwrap_or(usize::MAX)
}

pub fn entry(id: &str) -> &'static Override {
    OVERRIDES
        .iter()
        .find(|entry| entry.id == id)
        .unwrap_or(&NONE)
}
