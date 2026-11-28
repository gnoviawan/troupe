use super::*;
use std::os::unix::fs::{symlink, PermissionsExt};

#[test]
fn insecure_profile_ancestors_are_refused() {
    let root = std::env::temp_dir().join(format!("profile-security-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let profile = root.join("browser-host/profiles/user");
    // Valid even though `root` sits under `/tmp` (world-writable, root-owned):
    // validation starts at the trusted `base` (`root`) and never walks above it.
    prepare_profile(&root, &profile).unwrap();
    assert_eq!(
        std::fs::symlink_metadata(&profile)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );

    // Group-writable intermediate ancestor is refused.
    let ancestor = root.join("browser-host");
    std::fs::set_permissions(&ancestor, std::fs::Permissions::from_mode(0o770)).unwrap();
    assert!(prepare_profile(&root, &profile).is_err());
    std::fs::set_permissions(&ancestor, std::fs::Permissions::from_mode(0o700)).unwrap();

    // Symlink intermediate ancestor is refused.
    std::fs::rename(&ancestor, root.join("real")).unwrap();
    symlink(root.join("real"), &ancestor).unwrap();
    assert!(prepare_profile(&root, &profile).is_err());
    std::fs::remove_file(&ancestor).unwrap();
    std::fs::rename(root.join("real"), &ancestor).unwrap();

    // Group- or world-readable final profile directory is refused.
    std::fs::set_permissions(&profile, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(prepare_profile(&root, &profile).is_err());
    std::fs::set_permissions(&profile, std::fs::Permissions::from_mode(0o700)).unwrap();

    // Group-writable or symlink base is refused.
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o770)).unwrap();
    assert!(prepare_profile(&root, &profile).is_err());
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();

    let link_base =
        std::env::temp_dir().join(format!("profile-security-link-{}", uuid::Uuid::new_v4()));
    symlink(&root, &link_base).unwrap();
    assert!(prepare_profile(&link_base, &link_base.join("browser-host/profiles/user")).is_err());
    std::fs::remove_file(&link_base).unwrap();

    // Paths outside base, equal to base, or containing `..` are refused.
    assert!(prepare_profile(&root, &root).is_err());
    assert!(prepare_profile(&root, &root.join("browser-host/../escape")).is_err());
    assert!(prepare_profile(&root, &std::env::temp_dir().join("outside")).is_err());

    if unsafe { libc::geteuid() } == 0 {
        use std::os::unix::ffi::OsStrExt;
        let name = std::ffi::CString::new(ancestor.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::chown(name.as_ptr(), 65534, !0) }, 0);
        assert!(prepare_profile(&root, &profile).is_err());
    }
    std::fs::remove_dir_all(root).unwrap();
}
