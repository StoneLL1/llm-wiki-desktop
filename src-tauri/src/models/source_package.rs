use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const SOURCE_PACKAGE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourcePackageMemberRole {
    Index,
    Sheet,
    RowChunk,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourcePackageMember {
    pub order: u32,
    pub role: SourcePackageMemberRole,
    pub title: String,
    /// Staging-relative path before commit; retained as provenance afterwards.
    pub staging_path: String,
    pub wiki_path: String,
    pub baseline_path: String,
    pub content_hash: String,
    pub human_edit_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourcePackageManifest {
    pub schema_version: u32,
    pub source_id: String,
    pub version_id: String,
    pub entry_wiki_path: String,
    pub members: Vec<SourcePackageMember>,
}

impl SourcePackageManifest {
    pub fn staging(members: Vec<SourcePackageMember>) -> Self {
        Self {
            schema_version: SOURCE_PACKAGE_SCHEMA_VERSION,
            source_id: String::new(),
            version_id: String::new(),
            entry_wiki_path: String::new(),
            members,
        }
    }

    pub fn validate_staging(&self) -> Result<(), &'static str> {
        let mut staging_paths = HashSet::new();
        if self.schema_version != SOURCE_PACKAGE_SCHEMA_VERSION
            || !self.source_id.is_empty()
            || !self.version_id.is_empty()
            || !self.entry_wiki_path.is_empty()
            || self.members.is_empty()
            || self.members[0].role != SourcePackageMemberRole::Index
            || self.members.iter().enumerate().any(|(index, member)| {
                member.order as usize != index
                    || member.title.trim().is_empty()
                    || !safe_relative_path(&member.staging_path)
                    || !staging_paths.insert(member.staging_path.as_str())
                    || !member.wiki_path.is_empty()
                    || !member.baseline_path.is_empty()
                    || !valid_hash(&member.content_hash)
                    || member.human_edit_hash != member.content_hash
            })
        {
            return Err("SOURCE_PACKAGE_STAGING_INVALID");
        }
        Ok(())
    }

    pub fn validate_committed(&self) -> Result<(), &'static str> {
        self.validate_member_order()?;
        let mut staging_paths = HashSet::new();
        let mut wiki_paths = HashSet::new();
        let mut baseline_paths = HashSet::new();
        if self.source_id.is_empty()
            || self.version_id.is_empty()
            || self.entry_wiki_path.is_empty()
            || self.entry_wiki_path != self.members[0].wiki_path
            || self.members.iter().any(|member| {
                member.title.trim().is_empty()
                    || !safe_relative_path(&member.staging_path)
                    || !safe_relative_path(&member.wiki_path)
                    || !safe_relative_path(&member.baseline_path)
                    || !staging_paths.insert(member.staging_path.as_str())
                    || !wiki_paths.insert(member.wiki_path.as_str())
                    || !baseline_paths.insert(member.baseline_path.as_str())
                    || !valid_hash(&member.content_hash)
                    || !valid_hash(&member.human_edit_hash)
            })
        {
            return Err("SOURCE_PACKAGE_COMMITTED_INVALID");
        }
        Ok(())
    }

    fn validate_member_order(&self) -> Result<(), &'static str> {
        if self.schema_version != SOURCE_PACKAGE_SCHEMA_VERSION
            || self.members.is_empty()
            || self.members[0].role != SourcePackageMemberRole::Index
            || self
                .members
                .iter()
                .enumerate()
                .any(|(index, member)| member.order as usize != index)
        {
            return Err("SOURCE_PACKAGE_MEMBERS_INVALID");
        }
        Ok(())
    }
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn safe_relative_path(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('/')
        && !value.contains('\\')
        && value
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(
        order: u32,
        role: SourcePackageMemberRole,
        staging_path: &str,
    ) -> SourcePackageMember {
        let content_hash = "a".repeat(64);
        SourcePackageMember {
            order,
            role,
            title: format!("member-{order}"),
            staging_path: staging_path.to_owned(),
            wiki_path: String::new(),
            baseline_path: String::new(),
            human_edit_hash: content_hash.clone(),
            content_hash,
        }
    }

    #[test]
    fn staging_manifest_requires_unique_safe_ordered_members() {
        let valid = SourcePackageManifest::staging(vec![
            member(0, SourcePackageMemberRole::Index, "package/index.md"),
            member(1, SourcePackageMemberRole::Sheet, "package/sheet.md"),
        ]);
        assert_eq!(valid.validate_staging(), Ok(()));

        let mut duplicate = valid.clone();
        duplicate.members[1].staging_path = duplicate.members[0].staging_path.clone();
        assert_eq!(
            duplicate.validate_staging(),
            Err("SOURCE_PACKAGE_STAGING_INVALID")
        );

        let mut traversal = valid;
        traversal.members[1].staging_path = "../outside.md".to_owned();
        assert_eq!(
            traversal.validate_staging(),
            Err("SOURCE_PACKAGE_STAGING_INVALID")
        );
    }

    #[test]
    fn committed_manifest_binds_entry_and_rejects_duplicate_targets() {
        let mut valid = SourcePackageManifest::staging(vec![
            member(0, SourcePackageMemberRole::Index, "package/index.md"),
            member(1, SourcePackageMemberRole::RowChunk, "package/rows-1.md"),
        ]);
        valid.source_id = "source-1".to_owned();
        valid.version_id = "version-1".to_owned();
        for (index, member) in valid.members.iter_mut().enumerate() {
            member.wiki_path = format!("wiki/source/{index}.md");
            member.baseline_path = format!("raw/source/{index}.md");
        }
        valid.entry_wiki_path = valid.members[0].wiki_path.clone();
        assert_eq!(valid.validate_committed(), Ok(()));

        let mut wrong_entry = valid.clone();
        wrong_entry.entry_wiki_path = wrong_entry.members[1].wiki_path.clone();
        assert_eq!(
            wrong_entry.validate_committed(),
            Err("SOURCE_PACKAGE_COMMITTED_INVALID")
        );

        let mut duplicate = valid;
        duplicate.members[1].wiki_path = duplicate.members[0].wiki_path.clone();
        assert_eq!(
            duplicate.validate_committed(),
            Err("SOURCE_PACKAGE_COMMITTED_INVALID")
        );
    }
}
