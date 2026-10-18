use super::*;

// ---- second-round review fixes ----

#[test]
fn an_update_uninstall_keeps_the_downloaded_ffmpeg() {
    let nsi = include_str!("../../installer.nsi");
    // The silent uninstall of the old version always precedes a fresh install, so it
    // carries a keep marker, and the uninstall section reads it before deleting the tools.
    let call = nsi.find("StrCpy $R1 \"$R1 _?=$4\"").expect("uninstall call");
    let before = &nsi[..call];
    assert!(before.contains("/KEEPTOOLS"), "the pre-install uninstall passes the keep marker");
    let un = nsi.find("Section Uninstall").unwrap();
    let del = nsi[un..].find("ffmpeg.exe\"").expect("ffmpeg delete") + un;
    let guard = &nsi[un..del];
    assert!(guard.contains("$KeepTools"), "the ffmpeg delete is guarded by the keep marker");
    assert!(nsi.contains("GetOptions} $CMDLINE \"/KEEPTOOLS\""), "un.onInit reads the marker");
}
