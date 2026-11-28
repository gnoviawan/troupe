use super::*;
use std::os::unix::fs::{symlink, PermissionsExt};

#[test]
fn insecure_profile_ancestors_are_refused() {
    let root = std::env::temp_dir().join(format!("profile-security-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let profile = root.join("browser-host/profiles/user");
    prepare_profile(&profile).unwrap();
    let ancestor = root.join("browser-host");
    std::fs::set_permissions(&ancestor, std::fs::Permissions::from_mode(0o770)).unwrap();
    assert!(prepare_profile(&profile).is_err());
    std::fs::set_permissions(&ancestor, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::rename(&ancestor, root.join("real")).unwrap();
    symlink(root.join("real"), &ancestor).unwrap();
    assert!(prepare_profile(&profile).is_err());
    std::fs::remove_file(&ancestor).unwrap();
    std::fs::rename(root.join("real"), &ancestor).unwrap();
    if unsafe { libc::geteuid() } == 0 {
        use std::os::unix::ffi::OsStrExt;
        let name = std::ffi::CString::new(ancestor.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::chown(name.as_ptr(), 65534, !0) }, 0);
        assert!(prepare_profile(&profile).is_err());
    }
    std::fs::remove_dir_all(root).unwrap();
}
