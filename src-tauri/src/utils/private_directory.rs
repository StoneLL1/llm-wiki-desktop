use std::fs;
use std::io;
use std::path::Path;

pub(crate) fn ensure_private_directory(path: &Path) -> io::Result<()> {
    match fs::create_dir(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    validate_private_directory(path)
}

pub(crate) fn create_private_directory(path: &Path) -> io::Result<()> {
    fs::create_dir(path)?;
    validate_private_directory(path)
}

pub(crate) fn validate_private_directory(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata_is_link_or_reparse(&metadata) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "private workspace path is not a real directory",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
        let mode = fs::symlink_metadata(path)?.permissions().mode() & 0o777;
        if mode != 0o700 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "private workspace permissions could not be verified",
            ));
        }
    }
    Ok(())
}

#[cfg(windows)]
fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    metadata.file_type().is_symlink() || metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_directory_is_real_and_owner_only() {
        let parent = tempfile::tempdir().unwrap();
        let path = parent.path().join("private");
        create_private_directory(&path).unwrap();
        validate_private_directory(&path).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn private_directory_rejects_symlink() {
        use std::os::unix::fs::symlink;

        let parent = tempfile::tempdir().unwrap();
        let target = parent.path().join("target");
        fs::create_dir(&target).unwrap();
        let link = parent.path().join("link");
        symlink(&target, &link).unwrap();
        assert_eq!(
            validate_private_directory(&link).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
}
