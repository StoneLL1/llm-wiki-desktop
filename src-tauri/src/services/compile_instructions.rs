#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompilePromptRoute {
    ByokPlan,
    ByokManifest,
    Agent,
}

pub struct CompileInstructionSet {
    pub source_protection: &'static str,
    pub derived_page_policy: &'static str,
    pub source_traceability: &'static str,
    pub decision_rules: &'static str,
    pub structural_files: &'static str,
    pub no_delete_policy: &'static str,
}

pub fn shared_compile_instruction_set() -> CompileInstructionSet {
    CompileInstructionSet {
        source_protection: "wiki/sources/ holds verbatim imported originals. Read and cite these authoritative sources, but never create, modify, delete, recreate, summarize, or mirror any file under wiki/sources/.",
        derived_page_policy: "Generate only derived wiki pages under wiki/entities/, wiki/concepts/, wiki/synthesis/, or wiki/comparisons/. Derived pages synthesize across sources, avoid one-source-one-page summaries, and are named after the concept they cover, never after a source filename.",
        source_traceability: "Every derived page must cite sources two ways: frontmatter `sources: [\"<original-source-filename>\"]` listing every original used, and a human-readable `> Sources:` line with Markdown links or wikilinks to the originals.",
        decision_rules: "Decision Rules: create when a genuinely new concept, entity, synthesis, or comparison page is needed; update when new evidence materially changes an existing page; merge when a new source has the same core thesis as an existing derived page; add see-also cross-links when content spans related but distinct topics; annotate conflicts when sources disagree instead of hiding them; Cascade after material changes by scanning and updating affected pages before refreshing index and overview.",
        structural_files: "Maintain wiki/index.md, wiki/overview.md, and wiki/log.md. Compile output must not consist only of these structural pages; structural updates must reflect the derived pages changed by the compile.",
        no_delete_policy: "Never delete pages. If a page appears obsolete, record it in wiki/log.md for user review.",
    }
}

pub fn render_compile_core_instructions() -> String {
    let set = shared_compile_instruction_set();
    [
        set.source_protection,
        set.derived_page_policy,
        set.source_traceability,
        set.decision_rules,
        set.structural_files,
        set.no_delete_policy,
    ]
    .join("\n")
}

pub fn render_compile_prompt_header(route: CompilePromptRoute, language: &str) -> String {
    let route_header = match route {
        CompilePromptRoute::ByokPlan => {
            "Return only CompilePlan JSON matching {summary,items:[{action,targetPath,pageType,sourceIds,affectedExistingPages,reason,riskFlags}],globalRiskFlags}. Do not return Markdown files in this step. You only know the files included in this prompt; you have no filesystem or tool access."
        }
        CompilePromptRoute::ByokManifest => {
            "Return only CompileManifest JSON matching {files:[{path,content}],deletions:[],summary}. Follow the accepted CompilePlan exactly; do not invent extra file operations. You only know the files included in this prompt; you have no filesystem or tool access."
        }
        CompilePromptRoute::Agent => {
            "Compile this local Markdown wiki inside the supplied compile workspace. First write a validated plan to compile-plan.json, then write candidate Markdown files under wiki/. Work only inside this workspace root."
        }
    };
    format!(
        "{route_header}\n{}\n{}\nWrite each page's prose body in that language; keep frontmatter keys, file paths, page type values, and JSON structure in English.",
        render_compile_core_instructions(),
        crate::utils::i18n::language_instruction(language),
    )
}
