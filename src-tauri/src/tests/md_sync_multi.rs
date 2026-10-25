use super::*;
use std::sync::atomic::AtomicBool;

// ---- Two running copies of the app on one Markdown folder ----
//
// Each `SyncInst` is one app instance: its own library, its own memory of the folder (a
// `SyncState` the thread points at while the instance works) and its own clock. The two
// share one temp folder, exactly as two machines share a cloud folder. They are stepped one
// after the other through the real round (`sync_poll_core`, then the write the app's save
// hook queues), so nothing here is a model of the sync: it is the sync, driven from a test.
//
// Time is controlled: `Rig::tick` moves every instance's clock forward and no test sleeps.
// A scenario ends in `settle()`, which runs rounds until nothing changes any more (a folder
// that keeps changing is a loop between the instances and fails the test), and in
// `assert_converged()`.

pub(super) struct SyncInst {
    pub(super) name: &'static str,
    state: &'static SyncState,
    pub(super) db: Mutex<Store>,
    dirty: AtomicBool,
    replaced: Mutex<Vec<String>>,
}

struct RigHost<'a>(&'a SyncInst);

impl SyncRoundHost for RigHost<'_> {
    fn on_replace(&self, _settings: &Settings, old: &Prompt, _updated: &Prompt) {
        self.0.replaced.lock().unwrap().push(old.text.clone());
    }
    fn on_trashed(&self, _ids: &[String]) {}
    fn save(&self, _store: &Store, prompts_changed: bool, _settings_changed: bool, _trash_changed: bool) {
        // The app saves the prompts, and that save queues the folder write.
        if prompts_changed {
            self.0.dirty.store(true, Ordering::SeqCst);
        }
    }
    fn notify(&self) {}
}

impl SyncInst {
    fn new(name: &'static str) -> SyncInst {
        let mut settings = Settings::default();
        settings.ui_flags.insert("promptSync".to_string(), true);
        // A running app always has a first view.
        let mut first = test_view("main");
        first.name = "Main".to_string();
        settings.active_view = first.id.clone();
        settings.views.push(first);
        let store = Store { prompts: Vec::new(), trash: Vec::new(), settings, prompts_locked: false, written: Default::default() };
        SyncInst {
            name,
            // One per instance for the life of the test process, which is what the
            // thread-local override wants (`&'static`).
            state: Box::leak(Box::new(SyncState::default())),
            db: Mutex::new(store),
            dirty: AtomicBool::new(false),
            replaced: Mutex::new(Vec::new()),
        }
    }

    // Runs `f` as this instance: every sync call inside sees this instance's memory and clock.
    pub(super) fn run<R>(&self, f: impl FnOnce() -> R) -> R {
        SYNC_STATE_TEST.with(|c| c.set(Some(self.state)));
        let out = f();
        SYNC_STATE_TEST.with(|c| c.set(None));
        out
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Store> {
        self.db.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(super) fn now(&self) -> u64 {
        self.run(sync_now_secs)
    }

    pub(super) fn skew(&self, secs: i64) {
        self.state.clock_skew.fetch_add(secs, Ordering::SeqCst);
    }

    // What the app does when it starts with the feature on (see start_sync_poll).
    pub(super) fn start(&self, dir: &Path) {
        let (prompts, settings) = {
            let s = self.lock();
            (s.prompts.clone(), s.settings.clone())
        };
        self.run(|| sync_seed_seen(dir, &prompts, &settings));
    }

    // The app is closed and opened again: the library stays, what the instance remembered
    // about the folder does not.
    pub(super) fn restart(&mut self) {
        let fresh = SyncState::default();
        fresh.clock_skew.store(self.state.clock_skew.load(Ordering::SeqCst), Ordering::SeqCst);
        self.state = Box::leak(Box::new(fresh));
    }

    // The user switches the feature on (see set_ui_flag): the folder is written, then the
    // poll starts.
    pub(super) fn activate(&self, dir: &Path) -> usize {
        self.run(|| {
            sync_activate_core(&self.db, dir, &RigHost(self), &|prompts, settings| {
                sync_write_all_core(dir, prompts, settings, &|id| sync_live_in(&self.db, id))
            })
        })
    }

    pub(super) fn poll(&self, dir: &Path) {
        self.run(|| sync_poll_core(&self.db, dir, &RigHost(self)));
    }

    // The background flush the app's save queues.
    pub(super) fn flush(&self, dir: &Path) -> usize {
        let (prompts, settings) = {
            let s = self.lock();
            (s.prompts.clone(), s.settings.clone())
        };
        self.run(|| sync_write_all_core(dir, &prompts, &settings, &|id| sync_live_in(&self.db, id)))
    }

    // One poll interval: read the folder, then write if a save is pending.
    pub(super) fn step(&self, dir: &Path) {
        self.poll(dir);
        if self.dirty.swap(false, Ordering::SeqCst) {
            let failed = self.flush(dir);
            if failed > 0 {
                // A failed write is tried again with the next round.
                self.dirty.store(true, Ordering::SeqCst);
            }
        }
    }

    // ---- what the user does in the app ----

    pub(super) fn add(&self, name: &str, text: &str) -> String {
        let id = gen_id();
        let mut p = sync_blank_prompt(id.clone());
        p.name = name.to_string();
        p.text = text.to_string();
        self.lock().prompts.push(p);
        self.dirty.store(true, Ordering::SeqCst);
        id
    }

    pub(super) fn edit(&self, id: &str, f: impl FnOnce(&mut Prompt)) {
        let mut s = self.lock();
        let p = s.prompts.iter_mut().find(|p| p.id == id).unwrap_or_else(|| panic!("{}: no prompt {id}", self.name));
        f(p);
        drop(s);
        self.dirty.store(true, Ordering::SeqCst);
    }

    pub(super) fn settings(&self, f: impl FnOnce(&mut Settings)) {
        f(&mut self.lock().settings);
        self.dirty.store(true, Ordering::SeqCst);
    }

    pub(super) fn delete(&self, id: &str) {
        let now = self.now();
        let moved = delete_prompt_core(&mut self.lock(), id, now, |_, _| true).unwrap();
        assert!(moved, "{}: no prompt {id} to delete", self.name);
        self.dirty.store(true, Ordering::SeqCst);
    }

    // The user restores a prompt from the trash.
    pub(super) fn restore(&self, id: &str) {
        let mut s = self.lock();
        let pos = s.trash.iter().position(|p| p.id == id).unwrap_or_else(|| panic!("{}: no trashed prompt {id}", self.name));
        let mut p = s.trash.remove(pos);
        p.trashed_at = 0;
        s.prompts.push(p);
        drop(s);
        self.dirty.store(true, Ordering::SeqCst);
    }

    pub(super) fn get(&self, id: &str) -> Option<Prompt> {
        self.lock().prompts.iter().find(|p| p.id == id).cloned()
    }

    pub(super) fn in_trash(&self, id: &str) -> bool {
        self.lock().trash.iter().any(|p| p.id == id)
    }

    pub(super) fn count(&self) -> usize {
        self.lock().prompts.len()
    }

    fn view_of(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .lock()
            .prompts
            .iter()
            .map(|p| format!("{}|{}|{}|{}|{}|{:?}", p.id, p.name, p.text, p.color, p.notes, p.tags))
            .collect();
        v.sort();
        v
    }

    // Every text this instance still holds anywhere: library, trash, replaced versions.
    fn texts(&self) -> Vec<String> {
        let s = self.lock();
        let mut out: Vec<String> = s.prompts.iter().chain(s.trash.iter()).flat_map(|p| [p.name.clone(), p.text.clone()]).collect();
        out.extend(self.replaced.lock().unwrap().iter().cloned());
        out
    }
}

pub(super) struct Rig {
    pub(super) dir: PathBuf,
    pub(super) a: SyncInst,
    pub(super) b: SyncInst,
}

impl Rig {
    pub(super) fn new() -> Rig {
        let rig = Rig { dir: sync_tmp_dir("multi"), a: SyncInst::new("A"), b: SyncInst::new("B") };
        // Both instances start the way the app does with the feature on.
        rig.a.start(&rig.dir);
        rig.b.start(&rig.dir);
        rig
    }

    // Both clocks move on, so a file written a moment ago has settled.
    pub(super) fn tick(&self, secs: i64) {
        self.a.skew(secs);
        self.b.skew(secs);
    }

    // One poll interval for each instance, alternately.
    pub(super) fn cycle(&self) {
        self.tick(3);
        self.a.step(&self.dir);
        self.tick(3);
        self.b.step(&self.dir);
    }

    fn folder_state(&self) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(&self.dir)
            .map(|rd| {
                rd.flatten()
                    .map(|e| {
                        let bytes = fs::read(e.path()).unwrap_or_default();
                        format!("{}|{}", e.file_name().to_string_lossy(), sync_hash(&bytes))
                    })
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v
    }

    fn snapshot(&self) -> (Vec<String>, Vec<String>, Vec<String>) {
        (self.a.view_of(), self.b.view_of(), self.folder_state())
    }

    // Rounds until neither library nor the folder changes for three cycles in a row. A pair
    // that keeps rewriting each other's files never gets there.
    pub(super) fn settle(&self) -> usize {
        let mut last = self.snapshot();
        let mut calm = 0;
        for n in 1..=30 {
            self.cycle();
            let now = self.snapshot();
            if now == last {
                calm += 1;
                if calm >= 3 {
                    return n;
                }
            } else {
                calm = 0;
                last = now;
            }
        }
        panic!("the two instances never settled:\nA {:#?}\nB {:#?}\nfolder {:#?}", last.0, last.1, last.2);
    }

    // Names of the folder's prompt files (not copies, locks or the readme).
    pub(super) fn files(&self) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(&self.dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.ends_with(".md") && n != "AGENTS.md" && !n.contains(".conflict-"))
            .collect();
        v.sort();
        v
    }

    pub(super) fn conflict_copies(&self) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(&self.dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.contains(".conflict-"))
            .collect();
        v.sort();
        v
    }

    // Both libraries hold the same prompts with the same content, the folder holds one file
    // for each of them, and no lock is left behind.
    pub(super) fn assert_converged(&self) {
        assert_eq!(self.a.view_of(), self.b.view_of(), "the two libraries differ");
        let mut want: Vec<String> = self.a.lock().prompts.iter().map(|p| format!("{}.md", p.id)).collect();
        want.sort();
        assert_eq!(self.files(), want, "the folder does not hold exactly the library's files");
        let locks: Vec<String> = fs::read_dir(&self.dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.ends_with(".lock"))
            .collect();
        assert!(locks.is_empty(), "lock files left behind: {locks:?}");
    }

    // The text exists somewhere: a library, a trash, a replaced version or a file of the folder.
    pub(super) fn holds_text(&self, needle: &str) -> bool {
        if self.a.texts().iter().chain(self.b.texts().iter()).any(|t| t.contains(needle)) {
            return true;
        }
        fs::read_dir(&self.dir).unwrap().flatten().any(|e| String::from_utf8_lossy(&fs::read(e.path()).unwrap_or_default()).contains(needle))
    }
}

// Sets a file's modification time `secs` into the past.
pub(super) fn age_file(path: &Path, secs: u64) {
    let f = fs::OpenOptions::new().write(true).open(path).unwrap();
    f.set_modified(SystemTime::now() - Duration::from_secs(secs)).unwrap();
}

#[test]
fn multi_a_prompt_created_on_a_appears_on_b() {
    let rig = Rig::new();
    let id = rig.a.add("Greeting", "Hello there");
    rig.settle();
    let on_b = rig.b.get(&id).expect("B should have imported the prompt");
    assert_eq!(on_b.name, "Greeting");
    assert_eq!(on_b.text, "Hello there");
    rig.assert_converged();
}

#[test]
fn multi_an_edit_on_one_side_reaches_the_other() {
    let rig = Rig::new();
    let id = rig.a.add("Greeting", "Hello there");
    rig.settle();
    rig.b.edit(&id, |p| p.text = "Hello again".into());
    rig.settle();
    assert_eq!(rig.a.get(&id).unwrap().text, "Hello again");
    rig.assert_converged();
}

#[test]
fn multi_both_edit_the_same_prompt_in_one_round_and_no_text_is_lost() {
    let rig = Rig::new();
    let id = rig.a.add("Shared", "start");
    rig.settle();
    rig.a.edit(&id, |p| p.text = "version from A".into());
    rig.b.edit(&id, |p| p.text = "version from B".into());
    rig.settle();
    assert!(rig.holds_text("version from A"), "A's edit is gone");
    assert!(rig.holds_text("version from B"), "B's edit is gone");
    rig.assert_converged();
}

#[test]
fn multi_a_prompt_moved_to_another_folder_on_a_follows_on_b() {
    let rig = Rig::new();
    rig.a.settings(|s| s.views.push(test_view("v1")));
    let id = rig.a.add("Movable", "text");
    rig.a.settings(|s| {
        assert!(sync_join_tile_folder(s, &id, "Work", "#ff0000", "v1", Some([1, 1])));
    });
    rig.settle();
    let tile_folder = |inst: &SyncInst| {
        let s = inst.lock();
        sync_placement_for(&s.prompts[0], &s.settings).tile_folder
    };
    assert_eq!(tile_folder(&rig.b), "Work");
    rig.a.settings(|s| {
        assert!(sync_join_tile_folder(s, &id, "Home", "#00ff00", "v1", Some([3, 3])));
    });
    rig.settle();
    assert_eq!(tile_folder(&rig.b), "Home", "B did not follow the move");
    assert_eq!(tile_folder(&rig.a), "Home");
    rig.assert_converged();
}

#[test]
fn multi_two_prompts_with_the_same_title_stay_two_prompts() {
    let rig = Rig::new();
    let x = rig.a.add("Same title", "from A");
    let y = rig.b.add("Same title", "from B");
    rig.settle();
    assert_eq!(rig.a.count(), 2);
    assert_eq!(rig.b.count(), 2);
    assert_eq!(rig.a.get(&y).unwrap().text, "from B");
    assert_eq!(rig.b.get(&x).unwrap().text, "from A");
    rig.assert_converged();
}

#[test]
fn multi_a_file_cut_inside_its_header_is_never_imported() {
    let rig = Rig::new();
    rig.a.add("Real", "fine");
    rig.settle();
    fs::write(rig.dir.join("halfbaked.md"), "---\nname: Half\ncolor: \ntags: \nno").unwrap();
    rig.settle();
    assert!(rig.a.get("halfbaked").is_none() && rig.b.get("halfbaked").is_none());
    assert_eq!(rig.a.count(), 1);
}

#[test]
fn multi_two_hundred_prompts_cross_in_one_go() {
    let rig = Rig::new();
    for i in 0..200 {
        rig.a.add(&format!("Prompt {i}"), &format!("body {i}\nsecond line"));
    }
    rig.settle();
    assert_eq!(rig.b.count(), 200);
    rig.assert_converged();
}

#[test]
fn multi_a_rename_that_only_changes_the_case_crosses() {
    let rig = Rig::new();
    rig.a.settings(|s| s.views.push(test_view("v1")));
    let id = rig.a.add("hello world", "text");
    rig.a.settings(|s| {
        assert!(sync_join_tile_folder(s, &id, "work", "#ff0000", "v1", Some([1, 1])));
    });
    rig.settle();
    rig.a.edit(&id, |p| p.name = "Hello World".into());
    rig.a.settings(|s| {
        assert!(sync_join_tile_folder(s, &id, "Work", "#ff0000", "v1", Some([1, 1])));
    });
    rig.settle();
    assert_eq!(rig.b.get(&id).unwrap().name, "Hello World");
    let folders = rig.b.lock().settings.tile_folders.iter().map(|f| f.name.clone()).collect::<Vec<_>>();
    assert_eq!(folders.len(), 1, "a case-only rename made a second folder: {folders:?}");
    rig.assert_converged();
}

#[test]
fn multi_an_editor_that_saves_with_a_bom_is_read() {
    let rig = Rig::new();
    let id = rig.a.add("Plain", "before");
    rig.settle();
    let path = rig.dir.join(format!("{id}.md"));
    let text = fs::read_to_string(&path).unwrap().replace("before", "after the editor");
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(text.as_bytes());
    fs::write(&path, bytes).unwrap();
    rig.settle();
    assert_eq!(rig.a.get(&id).unwrap().text, "after the editor");
    assert_eq!(rig.b.get(&id).unwrap().text, "after the editor");
    rig.assert_converged();
}

#[test]
fn multi_a_file_in_another_encoding_is_kept_not_overwritten_silently() {
    let rig = Rig::new();
    let id = rig.a.add("Plain", "before");
    rig.settle();
    let path = rig.dir.join(format!("{id}.md"));
    // An editor that saved the file as UTF-16.
    let text = fs::read_to_string(&path).unwrap().replace("before", "utf sixteen words");
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend(text.encode_utf16().flat_map(|u| u.to_le_bytes()));
    fs::write(&path, &bytes).unwrap();
    rig.a.edit(&id, |p| p.text = "changed in the app".into());
    rig.settle();
    let kept = rig.conflict_copies().iter().any(|n| fs::read(rig.dir.join(n)).map(|b| b == bytes).unwrap_or(false));
    assert!(kept, "the file the poll could not read was overwritten without a copy");
    rig.assert_converged();
}

#[test]
fn multi_a_read_only_file_is_written_once_it_is_writable_again() {
    let rig = Rig::new();
    let id = rig.a.add("Guarded", "first");
    rig.settle();
    let path = rig.dir.join(format!("{id}.md"));
    let mut perm = fs::metadata(&path).unwrap().permissions();
    perm.set_readonly(true);
    fs::set_permissions(&path, perm.clone()).unwrap();
    rig.a.edit(&id, |p| p.text = "second".into());
    for _ in 0..3 {
        rig.cycle();
    }
    // Whatever happened meanwhile, nothing half-written may sit in the folder.
    let on_disk = fs::read_to_string(&path).unwrap();
    assert!(on_disk.ends_with('\n') && on_disk.starts_with("---\n"), "damaged file: {on_disk:?}");
    #[allow(clippy::permissions_set_readonly_false)]
    perm.set_readonly(false);
    fs::set_permissions(&path, perm).unwrap();
    rig.settle();
    assert_eq!(rig.b.get(&id).unwrap().text, "second");
    rig.assert_converged();
}

#[test]
fn multi_a_folder_that_vanishes_and_returns_loses_nothing() {
    let rig = Rig::new();
    let x = rig.a.add("One", "first");
    let y = rig.b.add("Two", "second");
    rig.settle();
    let hold = sync_tmp_dir("multi-hold");
    for e in fs::read_dir(&rig.dir).unwrap().flatten() {
        fs::copy(e.path(), hold.join(e.file_name())).unwrap();
    }
    fs::remove_dir_all(&rig.dir).unwrap();
    for _ in 0..8 {
        rig.cycle();
    }
    assert!(rig.a.get(&x).is_some() && rig.a.get(&y).is_some(), "A dropped prompts when the folder vanished");
    assert!(rig.b.get(&x).is_some() && rig.b.get(&y).is_some(), "B dropped prompts when the folder vanished");
    fs::create_dir_all(&rig.dir).unwrap();
    for e in fs::read_dir(&hold).unwrap().flatten() {
        fs::copy(e.path(), rig.dir.join(e.file_name())).unwrap();
    }
    rig.settle();
    assert_eq!(rig.a.count(), 2);
    rig.assert_converged();
}

#[test]
fn multi_a_folder_that_vanishes_is_rewritten_by_an_edit_and_the_other_side_follows() {
    let rig = Rig::new();
    let x = rig.a.add("One", "first");
    let y = rig.b.add("Two", "second");
    rig.settle();
    fs::remove_dir_all(&rig.dir).unwrap();
    rig.a.edit(&x, |p| p.text = "first, edited".into());
    rig.settle();
    assert_eq!(rig.b.get(&x).unwrap().text, "first, edited");
    assert!(rig.a.get(&y).is_some() && rig.b.get(&y).is_some());
    rig.assert_converged();
}

#[test]
fn multi_a_save_on_b_does_not_push_back_a_file_that_is_still_arriving() {
    let mut rig = Rig::new();
    let p1 = rig.a.add("One", "one");
    let p2 = rig.a.add("Two", "two");
    let p3 = rig.a.add("Three", "three");
    rig.settle();
    // A edits three prompts; over a cloud folder p3 lands and settles on B before p1 does.
    rig.a.edit(&p3, |p| p.text = "three from A".into());
    rig.tick(3);
    rig.a.step(&rig.dir);
    rig.tick(3);
    rig.b.poll(&rig.dir);
    rig.tick(3);
    rig.a.edit(&p1, |p| p.text = "one from A".into());
    rig.a.step(&rig.dir);
    // An unrelated save on B while p1 has not been read yet.
    rig.b.edit(&p2, |p| p.text = "two from B".into());
    rig.tick(3);
    rig.b.step(&rig.dir);
    rig.settle();
    assert_eq!(rig.b.get(&p1).unwrap().text, "one from A", "B reverted A's edit");
    assert_eq!(rig.a.get(&p1).unwrap().text, "one from A");
    assert_eq!(rig.a.get(&p3).unwrap().text, "three from A");
    assert_eq!(rig.a.get(&p2).unwrap().text, "two from B", "B's own change was lost");
    assert!(rig.conflict_copies().is_empty(), "a conflict copy appeared: {:?}", rig.conflict_copies());
    rig.assert_converged();
}

#[test]
fn multi_activating_with_an_older_library_does_not_push_it_back_into_the_folder() {
    let mut rig = Rig::new();
    let id = rig.a.add("Shared", "old text");
    rig.settle();
    // B's copy of the app is restored from an older state and switches the feature on
    // against a folder that already moved on.
    rig.b.restart();
    rig.a.edit(&id, |p| p.text = "new text".into());
    rig.tick(3);
    rig.a.step(&rig.dir);
    rig.tick(3);
    assert_eq!(rig.b.activate(&rig.dir), 0);
    rig.settle();
    assert_eq!(rig.b.get(&id).unwrap().text, "new text", "the folder's newer text was overwritten");
    assert_eq!(rig.a.get(&id).unwrap().text, "new text");
    assert!(rig.conflict_copies().is_empty(), "a conflict copy appeared: {:?}", rig.conflict_copies());
    rig.assert_converged();
}

#[test]
fn multi_a_prompt_deleted_on_a_is_gone_on_b_and_never_comes_back() {
    let rig = Rig::new();
    let id = rig.a.add("Doomed", "short lived");
    let keep = rig.a.add("Stays", "survivor");
    rig.settle();
    assert!(rig.b.get(&id).is_some());
    rig.a.delete(&id);
    rig.settle();
    assert!(rig.b.get(&id).is_none(), "B still has the deleted prompt");
    assert!(rig.a.get(&id).is_none(), "the deleted prompt came back on A");
    assert!(rig.b.in_trash(&id), "B should have trashed its copy, not dropped it");
    assert!(rig.b.get(&keep).is_some());
    rig.assert_converged();
}


#[test]
fn multi_b_edits_a_prompt_a_has_deleted_and_the_edit_survives() {
    let rig = Rig::new();
    let id = rig.a.add("Contested", "original");
    rig.settle();
    rig.a.delete(&id);
    rig.a.step(&rig.dir);
    rig.tick(3);
    // B has not seen the delete yet and edits.
    rig.b.edit(&id, |p| p.text = "edited after the delete".into());
    rig.settle();
    assert!(rig.holds_text("edited after the delete"), "B's edit was lost");
    // Whichever side wins, both end in the same state.
    rig.assert_converged();
}


#[test]
fn multi_clocks_ten_minutes_apart_still_converge() {
    let rig = Rig::new();
    rig.a.skew(-600);
    rig.b.skew(600);
    let id = rig.a.add("Skewed", "one");
    rig.settle();
    rig.b.edit(&id, |p| p.text = "two".into());
    rig.settle();
    rig.a.edit(&id, |p| p.text = "three".into());
    rig.settle();
    assert_eq!(rig.b.get(&id).unwrap().text, "three");
    let gone = rig.a.add("Skewed away", "x");
    rig.settle();
    rig.b.delete(&gone);
    rig.settle();
    assert!(rig.a.get(&gone).is_none(), "the delete did not cross the skew");
    rig.assert_converged();
}


#[test]
fn multi_a_restored_prompt_gets_its_file_back_and_b_does_not_sweep_it() {
    let rig = Rig::new();
    let id = rig.a.add("Back again", "restored text");
    rig.settle();
    rig.a.delete(&id);
    rig.settle();
    assert!(rig.b.in_trash(&id));
    rig.a.restore(&id);
    rig.settle();
    assert!(rig.a.get(&id).is_some(), "the restored prompt vanished again on A");
    assert!(rig.dir.join(format!("{id}.md")).exists(), "the restored prompt has no file");
    assert!(!rig.dir.join(format!("{id}.deleted")).exists(), "the old delete marker stayed and would trash it elsewhere");
    // B keeps its trash copy (a trashed prompt is never imported as new); restoring it there is
    // the user's move.
    assert!(rig.b.in_trash(&id));
    rig.b.restore(&id);
    rig.settle();
    rig.assert_converged();
}


#[test]
fn multi_a_delete_marker_expires_after_thirty_days_and_a_young_one_stays() {
    let rig = Rig::new();
    let id = rig.a.add("Gone", "x");
    rig.settle();
    rig.a.delete(&id);
    rig.settle();
    let tomb = rig.dir.join(format!("{id}.deleted"));
    assert!(tomb.exists(), "the delete left no marker");
    rig.tick(29 * 86400);
    rig.settle();
    assert!(tomb.exists(), "the marker went before its time");
    rig.tick(2 * 86400);
    rig.a.settings(|_| {});
    rig.settle();
    assert!(!tomb.exists(), "the marker outlived its thirty days");
    rig.assert_converged();
}


#[test]
fn multi_a_delete_marker_does_not_trash_a_confidential_prompt_of_the_same_id() {
    let rig = Rig::new();
    let id = rig.a.add("Shared", "x");
    rig.settle();
    rig.b.edit(&id, |p| p.secret = true);
    rig.a.delete(&id);
    rig.settle();
    assert!(rig.b.get(&id).is_some_and(|p| p.secret), "B's confidential prompt was trashed by a remote delete");
}


#[test]
fn multi_leaving_a_folder_removes_the_delete_markers_it_holds_and_names_a_stranger() {
    let rig = Rig::new();
    let id = rig.a.add("Gone", "x");
    rig.settle();
    rig.a.delete(&id);
    rig.settle();
    fs::write(rig.dir.join("abc.deleted"), "not a time").unwrap();
    let scan = sync_dir_scan(&rig.dir);
    assert_eq!(scan.foreign, vec!["abc.deleted".to_string()]);
    assert!(scan.ours.iter().any(|p| p.file_name().is_some_and(|n| n.to_string_lossy() == format!("{id}.deleted"))));
}
