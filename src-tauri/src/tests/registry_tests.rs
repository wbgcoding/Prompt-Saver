use crate::registry::{delete_tree, Hive, Key};

fn scratch_key(tag: &str) -> String {
    format!(r"Software\PromptSaverRegistryTest-{}-{tag}", std::process::id())
}

#[test]
fn registry_values_round_trip_and_missing_ones_read_as_not_found() {
    let path = scratch_key("a");
    let _ = delete_tree(Hive::CurrentUser, &path);
    assert_eq!(Key::open_read(Hive::CurrentUser, &path).err().map(|e| e.kind()), Some(std::io::ErrorKind::NotFound));
    {
        let key = Key::create_write(Hive::CurrentUser, &format!(r"{path}\sub\deeper")).unwrap();
        key.set_string("", "default value").unwrap();
        key.set_string("Text", "\"C:\\Apps\\x y.exe\" --minimized \u{e4}\u{20ac}").unwrap();
        key.set_u32("Flag", 1).unwrap();
        key.set_u32("Big", 0xFFFF_FFFE).unwrap();
        key.set_string("Long", &"x".repeat(5000)).unwrap();
    }
    let key = Key::open_read(Hive::CurrentUser, &format!(r"{path}\sub\deeper")).unwrap();
    assert_eq!(key.get_string("").unwrap(), "default value");
    assert_eq!(key.get_string("Text").unwrap(), "\"C:\\Apps\\x y.exe\" --minimized \u{e4}\u{20ac}");
    assert_eq!(key.get_u32("Flag").unwrap(), 1);
    assert_eq!(key.get_u32("Big").unwrap(), 0xFFFF_FFFE);
    assert_eq!(key.get_string("Long").unwrap().len(), 5000);
    // A value of the other type is an error, a missing one is NotFound.
    assert_eq!(key.get_u32("Text").err().map(|e| e.kind()), Some(std::io::ErrorKind::InvalidData));
    assert_eq!(key.get_string("Flag").err().map(|e| e.kind()), Some(std::io::ErrorKind::InvalidData));
    assert_eq!(key.get_string("Nope").err().map(|e| e.kind()), Some(std::io::ErrorKind::NotFound));
    drop(key);

    let w = Key::open_write(Hive::CurrentUser, &format!(r"{path}\sub\deeper")).unwrap();
    w.set_string("Text", "changed").unwrap();
    w.delete_value("Flag").unwrap();
    assert_eq!(w.delete_value("Flag").err().map(|e| e.kind()), Some(std::io::ErrorKind::NotFound));
    drop(w);
    let key = Key::open_read(Hive::CurrentUser, &format!(r"{path}\sub\deeper")).unwrap();
    assert_eq!(key.get_string("Text").unwrap(), "changed");
    assert_eq!(key.get_u32("Flag").err().map(|e| e.kind()), Some(std::io::ErrorKind::NotFound));
    drop(key);

    // The whole tree goes, parents included, and deleting it again is NotFound.
    delete_tree(Hive::CurrentUser, &path).unwrap();
    assert_eq!(Key::open_read(Hive::CurrentUser, &path).err().map(|e| e.kind()), Some(std::io::ErrorKind::NotFound));
    assert_eq!(delete_tree(Hive::CurrentUser, &path).err().map(|e| e.kind()), Some(std::io::ErrorKind::NotFound));
}

#[test]
fn the_machine_policy_key_can_be_read_without_elevation() {
    // Present on every Windows install; opening it read-only must work for a standard user.
    let key = Key::open_read(Hive::LocalMachine, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System").unwrap();
    // The value may be absent (default) but must never come back as anything but NotFound then.
    if let Err(e) = key.get_u32("ConsentPromptBehaviorUser") {
        assert_eq!(e.kind(), std::io::ErrorKind::NotFound);
    }
}
