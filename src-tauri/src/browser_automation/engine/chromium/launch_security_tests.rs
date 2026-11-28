use super::*;
use std::os::unix::fs::{symlink, PermissionsExt};
#[test]
fn profiles_are_private_and_symlinks_are_refused() {
    let root = std::env::temp_dir().join(format!("profile-security-{}", uuid::Uuid::new_v4()));
    let profile = root.join("profile");
    prepare_profile(&profile).unwrap();
    assert_eq!(
        std::fs::metadata(&profile).unwrap().permissions().mode() & 0o777,
        0o700
    );
    std::fs::set_permissions(&profile, std::fs::Permissions::from_mode(0o777)).unwrap();
    prepare_profile(&profile).unwrap();
    assert_eq!(
        std::fs::metadata(&profile).unwrap().permissions().mode() & 0o777,
        0o700
    );
    let link = root.join("link");
    symlink(&profile, &link).unwrap();
    assert!(prepare_profile(&link).is_err());
    std::fs::remove_dir_all(root).unwrap();
}
