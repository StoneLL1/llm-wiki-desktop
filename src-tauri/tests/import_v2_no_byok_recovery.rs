#[test]
fn import_v2_command_and_model_surfaces_do_not_expose_byok_recovery() {
    for (label, source) in [
        (
            "main commands",
            include_str!("../src/commands/import_v2_commands.rs"),
        ),
        (
            "Agent commands",
            include_str!("../src/commands/import_v2_agent_commands.rs"),
        ),
        ("registrations", include_str!("../src/lib.rs")),
        ("core models", include_str!("../src/models/import_v2.rs")),
        (
            "Agent models",
            include_str!("../src/models/import_v2_agent.rs"),
        ),
        (
            "presentation models",
            include_str!("../src/models/import_v2_presentation.rs"),
        ),
        (
            "Agent service",
            include_str!("../src/services/import_v2/agent_assistance.rs"),
        ),
        (
            "orchestrator",
            include_str!("../src/services/import_v2/orchestrator.rs"),
        ),
        (
            "Import view",
            include_str!("../../src/features/import/ImportView.tsx"),
        ),
        (
            "Import dialogs",
            include_str!("../../src/features/import/ImportV2Dialogs.tsx"),
        ),
        (
            "frontend workflow",
            include_str!("../../src/features/import/importWorkflow.ts"),
        ),
        (
            "frontend supporting actions",
            include_str!("../../src/features/import/useImportSupportingActions.ts"),
        ),
        (
            "frontend API",
            include_str!("../../src/services/importV2Api.ts"),
        ),
        ("frontend DTOs", include_str!("../../src/types/importV2.ts")),
        (
            "frontend Agent DTOs",
            include_str!("../../src/types/importV2Agent.ts"),
        ),
    ] {
        assert!(
            !source.to_ascii_lowercase().contains("byok"),
            "Import V2 {label} surface must not expose BYOK recovery"
        );
    }
}
