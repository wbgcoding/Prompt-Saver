Unicode true
ManifestDPIAware true
; Add in `dpiAwareness` `PerMonitorV2` to manifest for Windows 10 1607+ (note this should not affect lower versions since they should be able to ignore this and pick up `dpiAware` `true` set by `ManifestDPIAware true`)
; Currently undocumented on NSIS's website but is in the Docs folder of source tree, see
; https://github.com/kichik/nsis/blob/5fc0b87b819a9eec006df4967d08e522ddd651c9/Docs/src/attributes.but#L286-L300
; https://github.com/tauri-apps/tauri/pull/10106
ManifestDPIAwareness PerMonitorV2

!if "{{compression}}" == "none"
  SetCompress off
!else
  ; Set the compression algorithm. We default to LZMA.
  SetCompressor /SOLID "{{compression}}"
!endif

!include MUI2.nsh
!include FileFunc.nsh
!include x64.nsh
!include WordFunc.nsh
!include "utils.nsh"
!include "FileAssociation.nsh"
!include "Win\COM.nsh"
!include "Win\Propkey.nsh"
!include "StrFunc.nsh"
${StrCase}
${StrLoc}

{{#if installer_hooks}}
!include "{{installer_hooks}}"
{{/if}}

!define WEBVIEW2APPGUID "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"

!define MANUFACTURER "{{manufacturer}}"
!define PRODUCTNAME "{{product_name}}"
!define VERSION "{{version}}"
!define VERSIONWITHBUILD "{{version_with_build}}"
!define HOMEPAGE "{{homepage}}"
!define INSTALLMODE "{{install_mode}}"
!define LICENSE "{{license}}"
!define INSTALLERICON "{{installer_icon}}"
!define SIDEBARIMAGE "{{sidebar_image}}"
!define HEADERIMAGE "{{header_image}}"
!define UNINSTALLERICON "{{uninstaller_icon}}"
!define UNINSTALLERHEADERIMAGE "{{uninstaller_header_image}}"
!define MAINBINARYNAME "{{main_binary_name}}"
!define MAINBINARYSRCPATH "{{main_binary_path}}"
!define BUNDLEID "{{bundle_id}}"
!define COPYRIGHT "{{copyright}}"
!define OUTFILE "{{out_file}}"
!define ARCH "{{arch}}"
!define ADDITIONALPLUGINSPATH "{{additional_plugins_path}}"
!define ALLOWDOWNGRADES "{{allow_downgrades}}"
!define DISPLAYLANGUAGESELECTOR "{{display_language_selector}}"
!define INSTALLWEBVIEW2MODE "{{install_webview2_mode}}"
!define WEBVIEW2INSTALLERARGS "{{webview2_installer_args}}"
!define WEBVIEW2BOOTSTRAPPERPATH "{{webview2_bootstrapper_path}}"
!define WEBVIEW2INSTALLERPATH "{{webview2_installer_path}}"
!define MINIMUMWEBVIEW2VERSION "{{minimum_webview2_version}}"
!define UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCTNAME}"
!define MANUKEY "Software\${MANUFACTURER}"
!define MANUPRODUCTKEY "${MANUKEY}\${PRODUCTNAME}"
!define UNINSTALLERSIGNCOMMAND "{{uninstaller_sign_cmd}}"
!define ESTIMATEDSIZE "{{estimated_size}}"
!define STARTMENUFOLDER "{{start_menu_folder}}"
; Must match brand::AUTOSTART_KEY (src-tauri/src/brand.rs) — the app writes the Run
; value under this name, not ${PRODUCTNAME}. Stamped by tools/brand.js on rebrand.
!define AUTOSTARTKEY "PromptSaver"
; Must match brand::DATA_FOLDER (src-tauri/src/brand.rs), the folder under
; %APPDATA% that holds the database and settings. Stamped by tools/brand.js on
; rebrand. The uninstall data page and the delete step both read this one
; define instead of each spelling the folder name out separately.
!define DATAFOLDER "Prompt-Saver"
; Must match brand::TEMP_DIR and brand::TEMP_PREFIX (src-tauri/src/brand.rs): the
; scratch folder and the loose screenshot previews the app leaves in %TEMP%. The
; delete step removes both. Stamped by tools/brand.js on rebrand.
!define TEMPFOLDER "prompt-saver"
!define TEMPPREFIX "promptsaver"

Var PassiveMode
Var UpdateMode
Var NoShortcutMode
Var WixMode
Var OldMainBinaryName
Var PortableMode
Var PortableCheckbox

Name "${PRODUCTNAME}"
; Blank the footer instead of NSIS' default "Nullsoft Install System" branding.
!if "${COPYRIGHT}" != ""
  BrandingText "${COPYRIGHT}"
!else
  BrandingText " "
!endif
OutFile "${OUTFILE}"

; We don't actually use this value as default install path,
; it's just for nsis to append the product name folder in the directory selector
; https://nsis.sourceforge.io/Reference/InstallDir
!define PLACEHOLDER_INSTALL_DIR "placeholder\${PRODUCTNAME}"
InstallDir "${PLACEHOLDER_INSTALL_DIR}"

VIProductVersion "${VERSIONWITHBUILD}"
VIAddVersionKey "ProductName" "${PRODUCTNAME}"
VIAddVersionKey "FileDescription" "${PRODUCTNAME}"
VIAddVersionKey "LegalCopyright" "${COPYRIGHT}"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"

# additional plugins
!addplugindir "${ADDITIONALPLUGINSPATH}"

; Uninstaller signing command
!if "${UNINSTALLERSIGNCOMMAND}" != ""
  !uninstfinalize '${UNINSTALLERSIGNCOMMAND}'
!endif

; Handle install mode, `perUser`, `perMachine` or `both`
!if "${INSTALLMODE}" == "perMachine"
  RequestExecutionLevel admin
!endif

!if "${INSTALLMODE}" == "currentUser"
  RequestExecutionLevel user
!endif

!if "${INSTALLMODE}" == "both"
  !define MULTIUSER_MUI
  !define MULTIUSER_INSTALLMODE_INSTDIR "${PRODUCTNAME}"
  !define MULTIUSER_INSTALLMODE_COMMANDLINE
  !if "${ARCH}" == "x64"
    !define MULTIUSER_USE_PROGRAMFILES64
  !else if "${ARCH}" == "arm64"
    !define MULTIUSER_USE_PROGRAMFILES64
  !endif
  !define MULTIUSER_INSTALLMODE_DEFAULT_REGISTRY_KEY "${UNINSTKEY}"
  !define MULTIUSER_INSTALLMODE_DEFAULT_REGISTRY_VALUENAME "CurrentUser"
  !define MULTIUSER_INSTALLMODEPAGE_SHOWUSERNAME
  !define MULTIUSER_INSTALLMODE_FUNCTION RestorePreviousInstallLocation
  ; Admin, not Highest: "Highest" starts unelevated and RE-LAUNCHES the installer
  ; the moment "all users" is picked, which looks like the setup restarting itself
  ; mid-flow. Asking for elevation up front keeps it a single run (the all-users /
  ; just-me choice still works, it only decides the location + registry hive).
  !define MULTIUSER_EXECUTIONLEVEL Admin
  !include MultiUser.nsh
!endif

; Installer icon
!if "${INSTALLERICON}" != ""
  !define MUI_ICON "${INSTALLERICON}"
!endif

; Installer sidebar image
!if "${SIDEBARIMAGE}" != ""
  !define MUI_WELCOMEFINISHPAGE_BITMAP "${SIDEBARIMAGE}"
!endif

; Enable header images for installer and uninstaller pages when either image is configured.
!if "${HEADERIMAGE}" != ""
  !define MUI_HEADERIMAGE
!else if "${UNINSTALLERHEADERIMAGE}" != ""
  !define MUI_HEADERIMAGE
!endif

; Installer header image
!if "${HEADERIMAGE}" != ""
  !define MUI_HEADERIMAGE_BITMAP "${HEADERIMAGE}"
!endif

; Uninstaller header image
!if "${UNINSTALLERHEADERIMAGE}" != ""
  !define MUI_HEADERIMAGE_UNBITMAP "${UNINSTALLERHEADERIMAGE}"
!endif

; Uninstaller icon
!if "${UNINSTALLERICON}" != ""
  !define MUI_UNICON "${UNINSTALLERICON}"
!endif

; Define registry key to store installer language
!define MUI_LANGDLL_REGISTRY_ROOT "HKCU"
!define MUI_LANGDLL_REGISTRY_KEY "${MANUPRODUCTKEY}"
!define MUI_LANGDLL_REGISTRY_VALUENAME "Installer Language"

; Installer pages, must be ordered as they appear
; 1. Welcome Page
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
!insertmacro MUI_PAGE_WELCOME

; 2. License Page (if defined)
!if "${LICENSE}" != ""
  !define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
  !insertmacro MUI_PAGE_LICENSE "${LICENSE}"
!endif

; 3. Install mode (if it is set to `both`)
!if "${INSTALLMODE}" == "both"
  !define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
  !insertmacro MULTIUSER_PAGE_INSTALLMODE
!endif

; 4. Custom page to ask user if he wants to reinstall/uninstall
;    only if a previous installation was detected
Var ReinstallPageCheck
Page custom PageReinstall PageLeaveReinstall
Function PageReinstall
  ; Uninstall previous WiX installation if exists.
  ;
  ; A WiX installer stores the installation info in registry
  ; using a UUID and so we have to loop through all keys under
  ; `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall`
  ; and check if `DisplayName` and `Publisher` keys match ${PRODUCTNAME} and ${MANUFACTURER}
  ;
  ; This has a potential issue that there maybe another installation that matches
  ; our ${PRODUCTNAME} and ${MANUFACTURER} but wasn't installed by our WiX installer,
  ; however, this should be fine since the user will have to confirm the uninstallation
  ; and they can chose to abort it if doesn't make sense.
  StrCpy $0 0
  wix_loop:
    EnumRegKey $1 HKLM "SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall" $0
    StrCmp $1 "" wix_loop_done ; Exit loop if there is no more keys to loop on
    IntOp $0 $0 + 1
    ReadRegStr $R0 HKLM "SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\$1" "DisplayName"
    ReadRegStr $R1 HKLM "SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\$1" "Publisher"
    StrCmp "$R0$R1" "${PRODUCTNAME}${MANUFACTURER}" 0 wix_loop
    ReadRegStr $R0 HKLM "SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\$1" "UninstallString"
    ${StrCase} $R1 $R0 "L"
    ${StrLoc} $R0 $R1 "msiexec" ">"
    StrCmp $R0 0 0 wix_loop_done
    StrCpy $WixMode 1
    StrCpy $R6 "SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\$1"
    Goto compare_version
  wix_loop_done:

  ; Check if there is an existing installation, if not, abort the reinstall page
  ReadRegStr $R0 SHCTX "${UNINSTKEY}" ""
  ReadRegStr $R1 SHCTX "${UNINSTKEY}" "UninstallString"
  ${IfThen} "$R0$R1" == "" ${|} Abort ${|}

  ; Compare this installar version with the existing installation
  ; and modify the messages presented to the user accordingly
  compare_version:
  StrCpy $R4 "$(older)"
  ${If} $WixMode = 1
    ReadRegStr $R0 HKLM "$R6" "DisplayVersion"
  ${Else}
    ReadRegStr $R0 SHCTX "${UNINSTKEY}" "DisplayVersion"
  ${EndIf}
  ${IfThen} $R0 == "" ${|} StrCpy $R4 "$(unknown)" ${|}

  nsis_tauri_utils::SemverCompare "${VERSION}" $R0
  Pop $R0
  ; Reinstalling the same version
  ${If} $R0 = 0
    StrCpy $R1 "$(alreadyInstalledLong)"
    StrCpy $R2 "$(addOrReinstall)"
    StrCpy $R3 "$(uninstallApp)"
    !insertmacro MUI_HEADER_TEXT "$(alreadyInstalled)" "$(chooseMaintenanceOption)"
  ; Upgrading
  ${ElseIf} $R0 = 1
    StrCpy $R1 "$(olderOrUnknownVersionInstalled)"
    StrCpy $R2 "$(uninstallBeforeInstalling)"
    StrCpy $R3 "$(dontUninstall)"
    !insertmacro MUI_HEADER_TEXT "$(alreadyInstalled)" "$(choowHowToInstall)"
  ; Downgrading
  ${ElseIf} $R0 = -1
    StrCpy $R1 "$(newerVersionInstalled)"
    StrCpy $R2 "$(uninstallBeforeInstalling)"
    !if "${ALLOWDOWNGRADES}" == "true"
      StrCpy $R3 "$(dontUninstall)"
    !else
      StrCpy $R3 "$(dontUninstallDowngrade)"
    !endif
    !insertmacro MUI_HEADER_TEXT "$(alreadyInstalled)" "$(choowHowToInstall)"
  ${Else}
    Abort
  ${EndIf}

  ; Skip showing the page if passive.
  ; Upgrades and same-version reinstalls also skip it: the previous version is
  ; uninstalled automatically (user data is kept), see PageLeaveReinstall.
  ${If} $PassiveMode = 1
  ${OrIf} $R0 = 1
  ${OrIf} $R0 = 0
    Call PageLeaveReinstall
  ${Else}
    nsDialogs::Create 1018
    Pop $R4
    ${IfThen} $(^RTL) = 1 ${|} nsDialogs::SetRTL $(^RTL) ${|}

    ${NSD_CreateLabel} 0 2u 100% 42u $R1
    Pop $R1

    ; Same indent/size as the install-mode (choose-users) page radios.
    ${NSD_CreateRadioButton} 15u 50u -15u 10u $R2
    Pop $R2
    ${NSD_OnClick} $R2 PageReinstallUpdateSelection

    ${NSD_CreateRadioButton} 15u 70u -15u 10u $R3
    Pop $R3
    ; Disable this radio button if downgrading and downgrades are disabled
    !if "${ALLOWDOWNGRADES}" == "false"
      ${IfThen} $R0 = -1 ${|} EnableWindow $R3 0 ${|}
    !endif
    ${NSD_OnClick} $R3 PageReinstallUpdateSelection

    ; Check the first radio button if this the first time
    ; we enter this page or if the second button wasn't
    ; selected the last time we were on this page
    ${If} $ReinstallPageCheck <> 2
      SendMessage $R2 ${BM_SETCHECK} ${BST_CHECKED} 0
    ${Else}
      SendMessage $R3 ${BM_SETCHECK} ${BST_CHECKED} 0
    ${EndIf}

    ${NSD_SetFocus} $R2
    nsDialogs::Show
  ${EndIf}
FunctionEnd
Function PageReinstallUpdateSelection
  ${NSD_GetState} $R2 $R1
  ${If} $R1 == ${BST_CHECKED}
    StrCpy $ReinstallPageCheck 1
  ${Else}
    StrCpy $ReinstallPageCheck 2
  ${EndIf}
FunctionEnd
Function PageLeaveReinstall
  ${NSD_GetState} $R2 $R1

  ; If migrating from Wix, always uninstall
  ${If} $WixMode = 1
    Goto reinst_uninstall
  ${EndIf}

  ; In update mode, always proceeds without uninstalling
  ${If} $UpdateMode = 1
    Goto reinst_done
  ${EndIf}

  ; Upgrades AND same-version reinstalls always uninstall the previous version
  ; automatically, without asking. User data in AppData is not touched.
  ${If} $R0 = 1
  ${OrIf} $R0 = 0
    Goto reinst_uninstall
  ${EndIf}

  ; $R0 holds whether same(0)/upgrading(1)/downgrading(-1) version
  ; $R1 holds the radio buttons state:
  ;   1 => first choice was selected
  ;   0 => second choice was selected
  ${If} $R0 = 0 ; Same version, proceed
    ${If} $R1 = 1              ; User chose to add/reinstall
      Goto reinst_done
    ${Else}                    ; User chose to uninstall
      Goto reinst_uninstall
    ${EndIf}
  ${ElseIf} $R0 = 1 ; Upgrading
    ${If} $R1 = 1              ; User chose to uninstall
      Goto reinst_uninstall
    ${Else}
      Goto reinst_done         ; User chose NOT to uninstall
    ${EndIf}
  ${ElseIf} $R0 = -1 ; Downgrading
    ${If} $R1 = 1              ; User chose to uninstall
      Goto reinst_uninstall
    ${Else}
      Goto reinst_done         ; User chose NOT to uninstall
    ${EndIf}
  ${EndIf}

  reinst_uninstall:
    ; 95: HideWindow/BringToFront used to bracket the ExecWait below -- the old
    ; uninstaller runs with /S (no window of its own to overlap), so all that
    ; did was make the whole installer vanish for as long as the uninstall
    ; took, reading as "the installer closed" instead of "still working".
    ; The window now just stays up and on top the entire time; the header
    ; changes so it is visible that something is happening.
    !insertmacro MUI_HEADER_TEXT "$(removingPrevious)" ""
    DetailPrint "$(removingPrevious)"
    ClearErrors

    ${If} $WixMode = 1
      ReadRegStr $R1 HKLM "$R6" "UninstallString"
      ExecWait '$R1' $0
    ${Else}
      ReadRegStr $4 SHCTX "${MANUPRODUCTKEY}" ""
      ${If} $4 == ""
        ; 95: an empty product-location key must not turn into an empty _?=
        ; suffix -- that lets the uninstaller's own ExecWait return instantly
        ; while it is still deleting files in the background, so the fresh
        ; install below can start writing into the same folder mid-delete.
        ; Fall back to the uninstall key's own InstallLocation, then to this
        ; installer's own target dir; if even that is somehow empty, skip the
        ; automatic uninstall instead of running it blind with no directory.
        ReadRegStr $4 SHCTX "${UNINSTKEY}" "InstallLocation"
        ${IfThen} $4 == "" ${|} StrCpy $4 "$INSTDIR" ${|}
      ${EndIf}
      ${If} $4 == ""
        StrCpy $0 0
        Goto reinst_done
      ${EndIf}
      ReadRegStr $R1 SHCTX "${UNINSTKEY}" "UninstallString"
      ${IfThen} $UpdateMode = 1 ${|} StrCpy $R1 "$R1 /UPDATE" ${|} ; append /UPDATE
      ${IfThen} $PassiveMode = 1 ${|} StrCpy $R1 "$R1 /P" ${|} ; append /P
      StrCpy $R1 "$R1 /S" ; silent: the auto-uninstall of the old version shows no window (data kept)
      StrCpy $R1 "$R1 _?=$4" ; append uninstall directory -- forces ExecWait to actually wait
      ExecWait '$R1' $0
    ${EndIf}

    ${IfThen} ${Errors} ${|} StrCpy $0 2 ${|} ; ExecWait failed, set fake exit code

    ${If} $0 <> 0
    ${OrIf} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
      ; User cancelled wix uninstaller? return to select un/reinstall page
      ${If} $WixMode = 1
      ${AndIf} $0 = 1602
        Abort
      ${EndIf}

      ; User cancelled NSIS uninstaller? return to select un/reinstall page
      ${If} $0 = 1
        Abort
      ${EndIf}

      ; Other erros? show generic error message and return to select un/reinstall page
      MessageBox MB_ICONEXCLAMATION "$(unableToUninstall)"
      Abort
    ${EndIf}
  reinst_done:
FunctionEnd

; 5. Combined location + portable page (one page instead of two = fewer steps).
; Hidden for passive/update installs (those keep the existing location).
Var DirField
Page custom PageDir PageDirLeave
Function PageDir
  ${IfThen} $PassiveMode = 1 ${|} Abort ${|}
  ${IfThen} $UpdateMode = 1 ${|} Abort ${|}
  !insertmacro MUI_HEADER_TEXT "$(dirTitle)" "$(dirInfo)"
  nsDialogs::Create 1018
  Pop $0
  ${NSD_CreateLabel} 0 0 100% 20u "$(dirInfo)"
  Pop $0
  ${NSD_CreateText} 0 24u 76% 13u "$INSTDIR"
  Pop $DirField
  ${NSD_CreateButton} 78% 23u 22% 15u "$(^BrowseBtn)"
  Pop $0
  ${NSD_OnClick} $0 PageDirBrowse
  ${NSD_CreateCheckbox} 0 50u 100% 12u "$(portableCheck)"
  Pop $PortableCheckbox
  ${If} $PortableMode = 1
    ${NSD_Check} $PortableCheckbox
  ${EndIf}
  ${NSD_OnClick} $PortableCheckbox PageDirPortable
  ${NSD_CreateLabel} 0 66u 100% 40u "$(portableInfo)"
  Pop $0
  ; Last page before install: relabel the wizard's Next button to "Install".
  GetDlgItem $0 $HWNDPARENT 1
  SendMessage $0 ${WM_SETTEXT} 0 "STR:$(^InstallBtn)"
  nsDialogs::Show
FunctionEnd
Function PageDirBrowse
  ${NSD_GetText} $DirField $0
  nsDialogs::SelectFolderDialog "$(dirInfo)" "$0"
  Pop $1
  ${If} $1 != error
    ${NSD_SetText} $DirField "$1"
  ${EndIf}
FunctionEnd
; Portable on -> own subfolder next to this setup (never loose files in e.g.
; the Downloads folder); off -> standard path.
Function PageDirPortable
  ${NSD_GetState} $PortableCheckbox $0
  ${If} $0 = ${BST_CHECKED}
    ${NSD_SetText} $DirField "$EXEDIR\${PRODUCTNAME}"
  ${Else}
    ${NSD_SetText} $DirField "$INSTDIR"
  ${EndIf}
FunctionEnd
Function PageDirLeave
  ${NSD_GetText} $DirField $0
  ${If} $0 != ""
    StrCpy $INSTDIR "$0"
  ${EndIf}
  ${NSD_GetState} $PortableCheckbox $0
  ${If} $0 = ${BST_CHECKED}
    StrCpy $PortableMode 1
  ${Else}
    StrCpy $PortableMode 0
  ${EndIf}
FunctionEnd

; 6. Start menu shortcut page
Var AppStartMenuFolder
!if "${STARTMENUFOLDER}" != ""
  !define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
  !define MUI_STARTMENUPAGE_DEFAULTFOLDER "${STARTMENUFOLDER}"
!else
  !define MUI_PAGE_CUSTOMFUNCTION_PRE Skip
!endif
!insertmacro MUI_PAGE_STARTMENU Application $AppStartMenuFolder

; 7. Installation page
!insertmacro MUI_PAGE_INSTFILES

; 8. Finish page
;
; Don't auto jump to finish page after installation page,
; because the installation page has useful info that can be used debug any issues with the installer.
!define MUI_FINISHPAGE_NOAUTOCLOSE
; Use show readme button in the finish page as a button create a desktop shortcut
!define MUI_FINISHPAGE_SHOWREADME
!define MUI_FINISHPAGE_SHOWREADME_TEXT "$(createDesktop)"
!define MUI_FINISHPAGE_SHOWREADME_FUNCTION CreateOrUpdateDesktopShortcut
; Show run app after installation.
!define MUI_FINISHPAGE_RUN
!define MUI_FINISHPAGE_RUN_FUNCTION RunMainBinary
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
!insertmacro MUI_PAGE_FINISH

Function RunMainBinary
  nsis_tauri_utils::RunAsUser "$INSTDIR\${MAINBINARYNAME}.exe" ""
FunctionEnd

; Uninstaller Pages
; 1. Confirm uninstall page
!define MUI_PAGE_CUSTOMFUNCTION_PRE un.SkipIfPassive
!insertmacro MUI_UNPAGE_CONFIRM

; 1b. Data folder page: ask whether the saved prompts and settings stay or go.
; Default selection is always "keep". A passive uninstall skips this page the
; same way it skips the confirm page above, so an unattended run never deletes
; data without being told to.
Var DataDirPath
Var DataSizeStr
Var KeepDataRadio
Var DeleteDataRadio
Var DeleteDataState
UninstPage custom un.PageData un.PageDataLeave

Function un.FormatSizeKB
  ; In: $0 holds a size in KB. Out: $0 holds a short readable string.
  ${If} $0 < 1024
    StrCpy $0 "$0 KB"
  ${ElseIf} $0 < 1048576
    IntOp $1 $0 / 1024
    IntOp $2 $0 % 1024
    IntOp $2 $2 * 10
    IntOp $2 $2 / 1024
    StrCpy $0 "$1.$2 MB"
  ${Else}
    IntOp $1 $0 / 1048576
    IntOp $2 $0 % 1048576
    IntOp $2 $2 * 10
    IntOp $2 $2 / 1048576
    StrCpy $0 "$1.$2 GB"
  ${EndIf}
FunctionEnd

Function un.PageData
  ${IfThen} $PassiveMode = 1 ${|} Abort ${|}
  SetShellVarContext current
  ; Same define the delete step below uses, so the two never drift apart.
  StrCpy $DataDirPath "$APPDATA\${DATAFOLDER}"

  StrCpy $0 "0"
  ${If} ${FileExists} "$DataDirPath\*.*"
    ${GetSize} "$DataDirPath" "/S=0K" $0 $1 $2
  ${EndIf}
  Call un.FormatSizeKB
  StrCpy $DataSizeStr $0

  !insertmacro MUI_HEADER_TEXT "$(dataDirTitle)" "$(dataDirSubtitle)"
  nsDialogs::Create 1018
  Pop $0
  ${NSD_CreateLabel} 0 0 100% 24u "$(dataDirQuestion)"
  Pop $0

  ${NSD_CreateRadioButton} 10u 30u -10u 12u "$(dataDirKeep)"
  Pop $KeepDataRadio
  ${NSD_CreateRadioButton} 10u 46u -10u 12u "$(dataDirDelete)"
  Pop $DeleteDataRadio
  ; Keep is the standing default; only an explicit choice below changes it.
  SendMessage $KeepDataRadio ${BM_SETCHECK} ${BST_CHECKED} 0

  ${NSD_CreateLabel} 10u 66u -10u 30u "$(dataDirFolderLabel) $DataDirPath ($DataSizeStr)"
  Pop $0

  nsDialogs::Show
FunctionEnd

Function un.PageDataLeave
  ${NSD_GetState} $DeleteDataRadio $DeleteDataState
FunctionEnd

; 2. Uninstalling Page
!insertmacro MUI_UNPAGE_INSTFILES

;Languages
; Explicit list (mirrors bundle.windows.nsis.languages in tauri.conf.json).
; Turkish/Korean/Romanian stock NSIS files lack the MultiUser install-mode
; strings, so we pre-define them right before those languages load: the
; English fallback then finds them defined and stays silent (no compile
; warnings), while LANGFILE still builds a properly localized LangString.
!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "German"
!insertmacro MUI_LANGUAGE "Spanish"
!insertmacro MUI_LANGUAGE "French"
!insertmacro MUI_LANGUAGE "Italian"
!insertmacro MUI_LANGUAGE "PortugueseBR"
!insertmacro MUI_LANGUAGE "Polish"
!insertmacro MUI_LANGUAGE "Russian"
!insertmacro MUI_LANGUAGE "SimpChinese"
!insertmacro MUI_LANGUAGE "Japanese"
!insertmacro MUI_LANGUAGE "Dutch"
!define MULTIUSER_TEXT_INSTALLMODE_TITLE "Kullanıcıları Seçin"
!define MULTIUSER_TEXT_INSTALLMODE_SUBTITLE "$(^NameDA) uygulamasını hangi kullanıcılar için kurmak istediğinizi seçin."
!define MULTIUSER_INNERTEXT_INSTALLMODE_TOP "$(^NameDA) uygulamasını yalnızca kendiniz için mi yoksa bu bilgisayardaki tüm kullanıcılar için mi kurmak istediğinizi seçin. $(^ClickNext)"
!define MULTIUSER_INNERTEXT_INSTALLMODE_ALLUSERS "Bu bilgisayarı kullanan herkes (tüm kullanıcılar)"
!define MULTIUSER_INNERTEXT_INSTALLMODE_CURRENTUSER "Yalnızca benim için"
!insertmacro MUI_LANGUAGE "Turkish"
!define MULTIUSER_TEXT_INSTALLMODE_TITLE "사용자 선택"
!define MULTIUSER_TEXT_INSTALLMODE_SUBTITLE "$(^NameDA)을(를) 설치할 사용자를 선택하세요."
!define MULTIUSER_INNERTEXT_INSTALLMODE_TOP "$(^NameDA)을(를) 나만을 위해 설치할지 이 컴퓨터의 모든 사용자를 위해 설치할지 선택하세요. $(^ClickNext)"
!define MULTIUSER_INNERTEXT_INSTALLMODE_ALLUSERS "이 컴퓨터를 사용하는 모든 사용자"
!define MULTIUSER_INNERTEXT_INSTALLMODE_CURRENTUSER "나만"
!insertmacro MUI_LANGUAGE "Korean"
!insertmacro MUI_LANGUAGE "Hindi"
!insertmacro MUI_LANGUAGE "Indonesian"
!insertmacro MUI_LANGUAGE "Vietnamese"
!insertmacro MUI_LANGUAGE "Czech"
!insertmacro MUI_LANGUAGE "Ukrainian"
!insertmacro MUI_LANGUAGE "Swedish"
!define MULTIUSER_TEXT_INSTALLMODE_TITLE "Alegeți utilizatorii"
!define MULTIUSER_TEXT_INSTALLMODE_SUBTITLE "Alegeți pentru ce utilizatori doriți să instalați $(^NameDA)."
!define MULTIUSER_INNERTEXT_INSTALLMODE_TOP "Alegeți dacă doriți să instalați $(^NameDA) doar pentru dvs. sau pentru toți utilizatorii acestui computer. $(^ClickNext)"
!define MULTIUSER_INNERTEXT_INSTALLMODE_ALLUSERS "Oricine folosește acest computer (toți utilizatorii)"
!define MULTIUSER_INNERTEXT_INSTALLMODE_CURRENTUSER "Doar pentru mine"
!insertmacro MUI_LANGUAGE "Romanian"
!insertmacro MUI_RESERVEFILE_LANGDLL
{{#each language_files}}
  !include "{{this}}"
{{/each}}

; Location + portable page strings (DE + EN match the bundled installer languages).
LangString dirTitle ${LANG_ENGLISH} "Choose location"
LangString dirInfo ${LANG_ENGLISH} "Select the folder to install ${PRODUCTNAME} into:"
LangString portableSubtitle ${LANG_ENGLISH} "Where and how to install ${PRODUCTNAME}"
LangString portableCheck ${LANG_ENGLISH} "Portable Installation"
LangString portableInfo ${LANG_ENGLISH} "Portable mode skips the Start-menu, registry entries and the uninstaller — the app is simply placed in the folder above. Leave unchecked for a normal installation."
LangString dirTitle ${LANG_GERMAN} "Speicherort wählen"
LangString dirInfo ${LANG_GERMAN} "Wählen Sie den Ordner für die Installation von ${PRODUCTNAME}:"
LangString portableSubtitle ${LANG_GERMAN} "Wohin und wie ${PRODUCTNAME} installiert wird"
LangString portableCheck ${LANG_GERMAN} "Portable Installation"
LangString portableInfo ${LANG_GERMAN} "Der portable Modus verzichtet auf Startmenü, Registry-Einträge und den Deinstaller — die App wird einfach im oben gewählten Ordner abgelegt. Für eine normale Installation nicht ankreuzen."

LangString dirTitle ${LANG_SPANISH} "Elegir ubicación"
LangString dirInfo ${LANG_SPANISH} "Seleccione la carpeta de instalación:"
LangString portableCheck ${LANG_SPANISH} "Instalación portable"
LangString portableInfo ${LANG_SPANISH} "El modo portable no crea accesos directos ni entradas de registro: la app solo se coloca en la carpeta elegida."
LangString dirTitle ${LANG_FRENCH} "Choisir l'emplacement"
LangString dirInfo ${LANG_FRENCH} "Sélectionnez le dossier d'installation :"
LangString portableCheck ${LANG_FRENCH} "Installation portable"
LangString portableInfo ${LANG_FRENCH} "Le mode portable ne crée ni raccourcis ni entrées de registre - l'application est simplement placée dans le dossier choisi."
LangString dirTitle ${LANG_ITALIAN} "Scegli posizione"
LangString dirInfo ${LANG_ITALIAN} "Seleziona la cartella di installazione:"
LangString portableCheck ${LANG_ITALIAN} "Installazione portatile"
LangString portableInfo ${LANG_ITALIAN} "La modalità portatile non crea collegamenti né voci di registro: l'app viene solo inserita nella cartella scelta."
LangString dirTitle ${LANG_PORTUGUESEBR} "Escolher local"
LangString dirInfo ${LANG_PORTUGUESEBR} "Selecione a pasta de instalação:"
LangString portableCheck ${LANG_PORTUGUESEBR} "Instalação portátil"
LangString portableInfo ${LANG_PORTUGUESEBR} "O modo portátil não cria atalhos nem entradas de registro - o app é apenas colocado na pasta escolhida."
LangString dirTitle ${LANG_POLISH} "Wybierz lokalizację"
LangString dirInfo ${LANG_POLISH} "Wybierz folder instalacji:"
LangString portableCheck ${LANG_POLISH} "Instalacja przenośna"
LangString portableInfo ${LANG_POLISH} "Tryb przenośny nie tworzy skrótów ani wpisów rejestru - aplikacja jest tylko umieszczana w wybranym folderze."
LangString dirTitle ${LANG_RUSSIAN} "Выбор расположения"
LangString dirInfo ${LANG_RUSSIAN} "Выберите папку установки:"
LangString portableCheck ${LANG_RUSSIAN} "Портативная установка"
LangString portableInfo ${LANG_RUSSIAN} "Портативный режим не создаёт ярлыков и записей реестра - приложение просто помещается в выбранную папку."
LangString dirTitle ${LANG_SIMPCHINESE} "选择位置"
LangString dirInfo ${LANG_SIMPCHINESE} "选择安装文件夹："
LangString portableCheck ${LANG_SIMPCHINESE} "便携安装"
LangString portableInfo ${LANG_SIMPCHINESE} "便携模式不创建快捷方式或注册表项 - 应用只会放入所选文件夹。"
LangString dirTitle ${LANG_JAPANESE} "場所を選択"
LangString dirInfo ${LANG_JAPANESE} "インストール先フォルダーを選択:"
LangString portableCheck ${LANG_JAPANESE} "ポータブルインストール"
LangString portableInfo ${LANG_JAPANESE} "ポータブルモードはショートカットやレジストリ項目を作成せず、アプリを選択したフォルダーに置くだけです。"
LangString dirTitle ${LANG_DUTCH} "Locatie kiezen"
LangString dirInfo ${LANG_DUTCH} "Selecteer de installatiemap:"
LangString portableCheck ${LANG_DUTCH} "Draagbare installatie"
LangString portableInfo ${LANG_DUTCH} "De draagbare modus maakt geen snelkoppelingen of registervermeldingen - de app wordt alleen in de gekozen map geplaatst."
LangString dirTitle ${LANG_TURKISH} "Konum seç"
LangString dirInfo ${LANG_TURKISH} "Kurulum klasörünü seçin:"
LangString portableCheck ${LANG_TURKISH} "Taşınabilir kurulum"
LangString portableInfo ${LANG_TURKISH} "Taşınabilir mod kısayol veya kayıt defteri girdisi oluşturmaz - uygulama yalnızca seçilen klasöre yerleştirilir."
LangString dirTitle ${LANG_KOREAN} "위치 선택"
LangString dirInfo ${LANG_KOREAN} "설치 폴더 선택:"
LangString portableCheck ${LANG_KOREAN} "포터블 설치"
LangString portableInfo ${LANG_KOREAN} "포터블 모드는 바로 가기나 레지스트리 항목을 만들지 않고 앱을 선택한 폴더에 둡니다."
LangString dirTitle ${LANG_HINDI} "स्थान चुनें"
LangString dirInfo ${LANG_HINDI} "इंस्टॉल फ़ोल्डर चुनें:"
LangString portableCheck ${LANG_HINDI} "पोर्टेबल इंस्टॉलेशन"
LangString portableInfo ${LANG_HINDI} "पोर्टेबल मोड कोई शॉर्टकट या रजिस्ट्री प्रविष्टि नहीं बनाता - ऐप बस चुने हुए फ़ोल्डर में रखा जाता है।"
LangString dirTitle ${LANG_INDONESIAN} "Pilih lokasi"
LangString dirInfo ${LANG_INDONESIAN} "Pilih folder instalasi:"
LangString portableCheck ${LANG_INDONESIAN} "Instalasi portabel"
LangString portableInfo ${LANG_INDONESIAN} "Mode portabel tidak membuat pintasan atau entri registri - aplikasi hanya ditempatkan di folder yang dipilih."
LangString dirTitle ${LANG_VIETNAMESE} "Chọn vị trí"
LangString dirInfo ${LANG_VIETNAMESE} "Chọn thư mục cài đặt:"
LangString portableCheck ${LANG_VIETNAMESE} "Cài đặt di động"
LangString portableInfo ${LANG_VIETNAMESE} "Chế độ di động không tạo lối tắt hay mục registry - ứng dụng chỉ được đặt vào thư mục đã chọn."
LangString dirTitle ${LANG_CZECH} "Vybrat umístění"
LangString dirInfo ${LANG_CZECH} "Vyberte instalační složku:"
LangString portableCheck ${LANG_CZECH} "Přenosná instalace"
LangString portableInfo ${LANG_CZECH} "Přenosný režim nevytváří zástupce ani záznamy v registru - aplikace se pouze umístí do zvolené složky."
LangString dirTitle ${LANG_UKRAINIAN} "Вибір розташування"
LangString dirInfo ${LANG_UKRAINIAN} "Виберіть папку встановлення:"
LangString portableCheck ${LANG_UKRAINIAN} "Портативне встановлення"
LangString portableInfo ${LANG_UKRAINIAN} "Портативний режим не створює ярликів і записів реєстру - застосунок лише розміщується у вибраній папці."
LangString dirTitle ${LANG_SWEDISH} "Välj plats"
LangString dirInfo ${LANG_SWEDISH} "Välj installationsmapp:"
LangString portableCheck ${LANG_SWEDISH} "Portabel installation"
LangString portableInfo ${LANG_SWEDISH} "Portabelt läge skapar inga genvägar eller registerposter - appen placeras bara i den valda mappen."
LangString dirTitle ${LANG_ROMANIAN} "Alegeți locația"
LangString dirInfo ${LANG_ROMANIAN} "Selectați folderul de instalare:"
LangString portableCheck ${LANG_ROMANIAN} "Instalare portabilă"
LangString portableInfo ${LANG_ROMANIAN} "Modul portabil nu creează scurtături sau intrări de registru - aplicația este doar plasată în folderul ales."

; Data folder page strings (uninstaller): keep or delete the saved prompts and
; settings. Default selection is always "keep" (un.PageData).
LangString dataDirTitle ${LANG_ENGLISH} "Your data"
LangString dataDirSubtitle ${LANG_ENGLISH} "Choose what happens to your saved prompts and settings"
LangString dataDirQuestion ${LANG_ENGLISH} "Keep your saved prompts and settings, or delete them as well?"
LangString dataDirKeep ${LANG_ENGLISH} "Keep my data (recommended)"
LangString dataDirDelete ${LANG_ENGLISH} "Delete my data too"
LangString dataDirFolderLabel ${LANG_ENGLISH} "Folder:"
LangString dataDirTitle ${LANG_GERMAN} "Ihre Daten"
LangString dataDirSubtitle ${LANG_GERMAN} "Wählen Sie, was mit Ihren gespeicherten Prompts und Einstellungen geschieht"
LangString dataDirQuestion ${LANG_GERMAN} "Gespeicherte Prompts und Einstellungen behalten oder ebenfalls löschen?"
LangString dataDirKeep ${LANG_GERMAN} "Daten behalten (empfohlen)"
LangString dataDirDelete ${LANG_GERMAN} "Daten ebenfalls löschen"
LangString dataDirFolderLabel ${LANG_GERMAN} "Ordner:"
LangString dataDirTitle ${LANG_SPANISH} "Sus datos"
LangString dataDirSubtitle ${LANG_SPANISH} "Elija qué ocurre con sus prompts y ajustes guardados"
LangString dataDirQuestion ${LANG_SPANISH} "¿Conservar los prompts y ajustes guardados, o eliminarlos también?"
LangString dataDirKeep ${LANG_SPANISH} "Conservar mis datos (recomendado)"
LangString dataDirDelete ${LANG_SPANISH} "Eliminar también mis datos"
LangString dataDirFolderLabel ${LANG_SPANISH} "Carpeta:"
LangString dataDirTitle ${LANG_FRENCH} "Vos données"
LangString dataDirSubtitle ${LANG_FRENCH} "Choisissez ce qu'il advient de vos prompts et paramètres enregistrés"
LangString dataDirQuestion ${LANG_FRENCH} "Conserver les prompts et paramètres enregistrés, ou les supprimer aussi ?"
LangString dataDirKeep ${LANG_FRENCH} "Conserver mes données (recommandé)"
LangString dataDirDelete ${LANG_FRENCH} "Supprimer aussi mes données"
LangString dataDirFolderLabel ${LANG_FRENCH} "Dossier :"
LangString dataDirTitle ${LANG_ITALIAN} "I tuoi dati"
LangString dataDirSubtitle ${LANG_ITALIAN} "Scegli cosa succede ai prompt e alle impostazioni salvate"
LangString dataDirQuestion ${LANG_ITALIAN} "Mantenere i prompt e le impostazioni salvate, oppure eliminarli anche?"
LangString dataDirKeep ${LANG_ITALIAN} "Mantieni i miei dati (consigliato)"
LangString dataDirDelete ${LANG_ITALIAN} "Elimina anche i miei dati"
LangString dataDirFolderLabel ${LANG_ITALIAN} "Cartella:"
LangString dataDirTitle ${LANG_PORTUGUESEBR} "Seus dados"
LangString dataDirSubtitle ${LANG_PORTUGUESEBR} "Escolha o que acontece com os prompts e configurações salvos"
LangString dataDirQuestion ${LANG_PORTUGUESEBR} "Manter os prompts e configurações salvos, ou excluí-los também?"
LangString dataDirKeep ${LANG_PORTUGUESEBR} "Manter meus dados (recomendado)"
LangString dataDirDelete ${LANG_PORTUGUESEBR} "Excluir meus dados também"
LangString dataDirFolderLabel ${LANG_PORTUGUESEBR} "Pasta:"
LangString dataDirTitle ${LANG_POLISH} "Twoje dane"
LangString dataDirSubtitle ${LANG_POLISH} "Wybierz, co ma się stać z zapisanymi promptami i ustawieniami"
LangString dataDirQuestion ${LANG_POLISH} "Zachować zapisane prompty i ustawienia, czy również je usunąć?"
LangString dataDirKeep ${LANG_POLISH} "Zachowaj moje dane (zalecane)"
LangString dataDirDelete ${LANG_POLISH} "Usuń również moje dane"
LangString dataDirFolderLabel ${LANG_POLISH} "Folder:"
LangString dataDirTitle ${LANG_RUSSIAN} "Ваши данные"
LangString dataDirSubtitle ${LANG_RUSSIAN} "Выберите, что сделать с сохранёнными запросами и настройками"
LangString dataDirQuestion ${LANG_RUSSIAN} "Сохранить запросы и настройки, или удалить их тоже?"
LangString dataDirKeep ${LANG_RUSSIAN} "Сохранить мои данные (рекомендуется)"
LangString dataDirDelete ${LANG_RUSSIAN} "Удалить мои данные тоже"
LangString dataDirFolderLabel ${LANG_RUSSIAN} "Папка:"
LangString dataDirTitle ${LANG_SIMPCHINESE} "您的数据"
LangString dataDirSubtitle ${LANG_SIMPCHINESE} "选择如何处理已保存的提示词和设置"
LangString dataDirQuestion ${LANG_SIMPCHINESE} "保留已保存的提示词和设置，还是一并删除？"
LangString dataDirKeep ${LANG_SIMPCHINESE} "保留我的数据（推荐）"
LangString dataDirDelete ${LANG_SIMPCHINESE} "同时删除我的数据"
LangString dataDirFolderLabel ${LANG_SIMPCHINESE} "文件夹："
LangString dataDirTitle ${LANG_JAPANESE} "データについて"
LangString dataDirSubtitle ${LANG_JAPANESE} "保存したプロンプトと設定をどうするか選択してください"
LangString dataDirQuestion ${LANG_JAPANESE} "保存したプロンプトと設定を残しますか、それとも削除しますか？"
LangString dataDirKeep ${LANG_JAPANESE} "データを残す（推奨）"
LangString dataDirDelete ${LANG_JAPANESE} "データも削除する"
LangString dataDirFolderLabel ${LANG_JAPANESE} "フォルダー:"
LangString dataDirTitle ${LANG_DUTCH} "Uw gegevens"
LangString dataDirSubtitle ${LANG_DUTCH} "Kies wat er met uw opgeslagen prompts en instellingen gebeurt"
LangString dataDirQuestion ${LANG_DUTCH} "Opgeslagen prompts en instellingen behouden, of ook verwijderen?"
LangString dataDirKeep ${LANG_DUTCH} "Mijn gegevens behouden (aanbevolen)"
LangString dataDirDelete ${LANG_DUTCH} "Mijn gegevens ook verwijderen"
LangString dataDirFolderLabel ${LANG_DUTCH} "Map:"
LangString dataDirTitle ${LANG_TURKISH} "Verileriniz"
LangString dataDirSubtitle ${LANG_TURKISH} "Kaydedilen istemler ve ayarlara ne olacağını seçin"
LangString dataDirQuestion ${LANG_TURKISH} "Kaydedilen istemler ve ayarlar korunsun mu, yoksa silinsin mi?"
LangString dataDirKeep ${LANG_TURKISH} "Verilerimi koru (önerilen)"
LangString dataDirDelete ${LANG_TURKISH} "Verilerimi de sil"
LangString dataDirFolderLabel ${LANG_TURKISH} "Klasör:"
LangString dataDirTitle ${LANG_KOREAN} "내 데이터"
LangString dataDirSubtitle ${LANG_KOREAN} "저장된 프롬프트와 설정을 어떻게 할지 선택하세요"
LangString dataDirQuestion ${LANG_KOREAN} "저장된 프롬프트와 설정을 유지할까요, 아니면 함께 삭제할까요?"
LangString dataDirKeep ${LANG_KOREAN} "데이터 유지 (권장)"
LangString dataDirDelete ${LANG_KOREAN} "데이터도 함께 삭제"
LangString dataDirFolderLabel ${LANG_KOREAN} "폴더:"
LangString dataDirTitle ${LANG_HINDI} "आपका डेटा"
LangString dataDirSubtitle ${LANG_HINDI} "चुनें कि सहेजे गए प्रॉम्प्ट और सेटिंग्स का क्या होगा"
LangString dataDirQuestion ${LANG_HINDI} "सहेजे गए प्रॉम्प्ट और सेटिंग्स रखें, या उन्हें भी हटा दें?"
LangString dataDirKeep ${LANG_HINDI} "मेरा डेटा रखें (अनुशंसित)"
LangString dataDirDelete ${LANG_HINDI} "मेरा डेटा भी हटाएं"
LangString dataDirFolderLabel ${LANG_HINDI} "फ़ोल्डर:"
LangString dataDirTitle ${LANG_INDONESIAN} "Data Anda"
LangString dataDirSubtitle ${LANG_INDONESIAN} "Pilih apa yang terjadi pada prompt dan pengaturan yang tersimpan"
LangString dataDirQuestion ${LANG_INDONESIAN} "Simpan prompt dan pengaturan yang tersimpan, atau hapus juga?"
LangString dataDirKeep ${LANG_INDONESIAN} "Simpan data saya (disarankan)"
LangString dataDirDelete ${LANG_INDONESIAN} "Hapus data saya juga"
LangString dataDirFolderLabel ${LANG_INDONESIAN} "Folder:"
LangString dataDirTitle ${LANG_VIETNAMESE} "Dữ liệu của bạn"
LangString dataDirSubtitle ${LANG_VIETNAMESE} "Chọn điều gì xảy ra với các prompt và cài đặt đã lưu"
LangString dataDirQuestion ${LANG_VIETNAMESE} "Giữ lại các prompt và cài đặt đã lưu, hay xóa luôn?"
LangString dataDirKeep ${LANG_VIETNAMESE} "Giữ dữ liệu của tôi (khuyên dùng)"
LangString dataDirDelete ${LANG_VIETNAMESE} "Xóa luôn dữ liệu của tôi"
LangString dataDirFolderLabel ${LANG_VIETNAMESE} "Thư mục:"
LangString dataDirTitle ${LANG_CZECH} "Vaše data"
LangString dataDirSubtitle ${LANG_CZECH} "Vyberte, co se stane s uloženými prompty a nastavením"
LangString dataDirQuestion ${LANG_CZECH} "Zachovat uložené prompty a nastavení, nebo je také smazat?"
LangString dataDirKeep ${LANG_CZECH} "Zachovat má data (doporučeno)"
LangString dataDirDelete ${LANG_CZECH} "Smazat i má data"
LangString dataDirFolderLabel ${LANG_CZECH} "Složka:"
LangString dataDirTitle ${LANG_UKRAINIAN} "Ваші дані"
LangString dataDirSubtitle ${LANG_UKRAINIAN} "Виберіть, що станеться зі збереженими запитами та налаштуваннями"
LangString dataDirQuestion ${LANG_UKRAINIAN} "Зберегти збережені запити та налаштування, чи видалити їх також?"
LangString dataDirKeep ${LANG_UKRAINIAN} "Зберегти мої дані (рекомендовано)"
LangString dataDirDelete ${LANG_UKRAINIAN} "Видалити також мої дані"
LangString dataDirFolderLabel ${LANG_UKRAINIAN} "Папка:"
LangString dataDirTitle ${LANG_SWEDISH} "Din data"
LangString dataDirSubtitle ${LANG_SWEDISH} "Välj vad som ska hända med sparade promptar och inställningar"
LangString dataDirQuestion ${LANG_SWEDISH} "Behåll sparade promptar och inställningar, eller ta bort dem också?"
LangString dataDirKeep ${LANG_SWEDISH} "Behåll min data (rekommenderas)"
LangString dataDirDelete ${LANG_SWEDISH} "Ta bort min data också"
LangString dataDirFolderLabel ${LANG_SWEDISH} "Mapp:"
LangString dataDirTitle ${LANG_ROMANIAN} "Datele dumneavoastră"
LangString dataDirSubtitle ${LANG_ROMANIAN} "Alegeți ce se întâmplă cu prompturile și setările salvate"
LangString dataDirQuestion ${LANG_ROMANIAN} "Păstrați prompturile și setările salvate, sau le ștergeți și pe acestea?"
LangString dataDirKeep ${LANG_ROMANIAN} "Păstrează datele mele (recomandat)"
LangString dataDirDelete ${LANG_ROMANIAN} "Șterge și datele mele"
LangString dataDirFolderLabel ${LANG_ROMANIAN} "Folder:"

; 95: header shown while the previous version is silently removed during an
; upgrade/reinstall (reinst_uninstall) -- the window itself stays visible and
; on top the whole time now, this just says what it is waiting on.
LangString removingPrevious ${LANG_ENGLISH} "Removing the previous version"
LangString removingPrevious ${LANG_GERMAN} "Vorherige Version wird entfernt"
LangString removingPrevious ${LANG_SPANISH} "Eliminando la versión anterior"
LangString removingPrevious ${LANG_FRENCH} "Suppression de la version précédente"
LangString removingPrevious ${LANG_ITALIAN} "Rimozione della versione precedente"
LangString removingPrevious ${LANG_PORTUGUESEBR} "Removendo a versão anterior"
LangString removingPrevious ${LANG_POLISH} "Usuwanie poprzedniej wersji"
LangString removingPrevious ${LANG_RUSSIAN} "Удаление предыдущей версии"
LangString removingPrevious ${LANG_SIMPCHINESE} "正在删除旧版本"
LangString removingPrevious ${LANG_JAPANESE} "以前のバージョンを削除しています"
LangString removingPrevious ${LANG_DUTCH} "Vorige versie wordt verwijderd"
LangString removingPrevious ${LANG_TURKISH} "Önceki sürüm kaldırılıyor"
LangString removingPrevious ${LANG_KOREAN} "이전 버전 제거 중"
LangString removingPrevious ${LANG_HINDI} "पिछला संस्करण हटाया जा रहा है"
LangString removingPrevious ${LANG_INDONESIAN} "Menghapus versi sebelumnya"
LangString removingPrevious ${LANG_VIETNAMESE} "Đang gỡ bỏ phiên bản trước"
LangString removingPrevious ${LANG_CZECH} "Odebírání předchozí verze"
LangString removingPrevious ${LANG_UKRAINIAN} "Видалення попередньої версії"
LangString removingPrevious ${LANG_SWEDISH} "Tar bort tidigare version"
LangString removingPrevious ${LANG_ROMANIAN} "Se elimină versiunea anterioară"

Function .onInit
  ${GetOptions} $CMDLINE "/P" $PassiveMode
  ${IfNot} ${Errors}
    StrCpy $PassiveMode 1
  ${EndIf}

  ${GetOptions} $CMDLINE "/NS" $NoShortcutMode
  ${IfNot} ${Errors}
    StrCpy $NoShortcutMode 1
  ${EndIf}

  ${GetOptions} $CMDLINE "/UPDATE" $UpdateMode
  ${IfNot} ${Errors}
    StrCpy $UpdateMode 1
  ${EndIf}

  !if "${DISPLAYLANGUAGESELECTOR}" == "true"
    !insertmacro MUI_LANGDLL_DISPLAY
  !endif

  !insertmacro SetContext

  ${If} $INSTDIR == "${PLACEHOLDER_INSTALL_DIR}"
    ; Set default install location
    !if "${INSTALLMODE}" == "perMachine"
      ${If} ${RunningX64}
        !if "${ARCH}" == "x64"
          StrCpy $INSTDIR "$PROGRAMFILES64\${PRODUCTNAME}"
        !else if "${ARCH}" == "arm64"
          StrCpy $INSTDIR "$PROGRAMFILES64\${PRODUCTNAME}"
        !else
          StrCpy $INSTDIR "$PROGRAMFILES\${PRODUCTNAME}"
        !endif
      ${Else}
        StrCpy $INSTDIR "$PROGRAMFILES\${PRODUCTNAME}"
      ${EndIf}
    !else if "${INSTALLMODE}" == "currentUser"
      StrCpy $INSTDIR "$LOCALAPPDATA\${PRODUCTNAME}"
    !endif

    Call RestorePreviousInstallLocation
  ${EndIf}


  !if "${INSTALLMODE}" == "both"
    !insertmacro MULTIUSER_INIT
  !endif
FunctionEnd


Section EarlyChecks
  ; Abort silent installer if downgrades is disabled
  !if "${ALLOWDOWNGRADES}" == "false"
  ${If} ${Silent}
    ; If downgrading
    ${If} $R0 = -1
      System::Call 'kernel32::AttachConsole(i -1)i.r0'
      ${If} $0 <> 0
        System::Call 'kernel32::GetStdHandle(i -11)i.r0'
        System::call 'kernel32::SetConsoleTextAttribute(i r0, i 0x0004)' ; set red color
        FileWrite $0 "$(silentDowngrades)"
      ${EndIf}
      Abort
    ${EndIf}
  ${EndIf}
  !endif

SectionEnd

Section WebView2
  ; Check if Webview2 is already installed and skip this section
  ${If} ${RunningX64}
    ReadRegStr $4 HKLM "SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\${WEBVIEW2APPGUID}" "pv"
  ${Else}
    ReadRegStr $4 HKLM "SOFTWARE\Microsoft\EdgeUpdate\Clients\${WEBVIEW2APPGUID}" "pv"
  ${EndIf}
  ${If} $4 == ""
    ReadRegStr $4 HKCU "SOFTWARE\Microsoft\EdgeUpdate\Clients\${WEBVIEW2APPGUID}" "pv"
  ${EndIf}

  ${If} $4 == ""
    ; Webview2 installation
    ;
    ; Skip if updating
    ${If} $UpdateMode <> 1
      !if "${INSTALLWEBVIEW2MODE}" == "downloadBootstrapper"
        Delete "$TEMP\MicrosoftEdgeWebview2Setup.exe"
        DetailPrint "$(webview2Downloading)"
        NSISdl::download "https://go.microsoft.com/fwlink/p/?LinkId=2124703" "$TEMP\MicrosoftEdgeWebview2Setup.exe"
        Pop $0
        ${If} $0 == "success"
          DetailPrint "$(webview2DownloadSuccess)"
        ${Else}
          DetailPrint "$(webview2DownloadError)"
          Abort "$(webview2AbortError)"
        ${EndIf}
        StrCpy $6 "$TEMP\MicrosoftEdgeWebview2Setup.exe"
        Goto install_webview2
      !endif

      !if "${INSTALLWEBVIEW2MODE}" == "embedBootstrapper"
        Delete "$TEMP\MicrosoftEdgeWebview2Setup.exe"
        File "/oname=$TEMP\MicrosoftEdgeWebview2Setup.exe" "${WEBVIEW2BOOTSTRAPPERPATH}"
        DetailPrint "$(installingWebview2)"
        StrCpy $6 "$TEMP\MicrosoftEdgeWebview2Setup.exe"
        Goto install_webview2
      !endif

      !if "${INSTALLWEBVIEW2MODE}" == "offlineInstaller"
        Delete "$TEMP\MicrosoftEdgeWebView2RuntimeInstaller.exe"
        File "/oname=$TEMP\MicrosoftEdgeWebView2RuntimeInstaller.exe" "${WEBVIEW2INSTALLERPATH}"
        DetailPrint "$(installingWebview2)"
        StrCpy $6 "$TEMP\MicrosoftEdgeWebView2RuntimeInstaller.exe"
        Goto install_webview2
      !endif

      Goto webview2_done

      install_webview2:
        DetailPrint "$(installingWebview2)"
        ; $6 holds the path to the webview2 installer
        ExecWait "$6 ${WEBVIEW2INSTALLERARGS} /install" $1
        ${If} $1 = 0
          DetailPrint "$(webview2InstallSuccess)"
        ${Else}
          DetailPrint "$(webview2InstallError)"
          Abort "$(webview2AbortError)"
        ${EndIf}
      webview2_done:
    ${EndIf}
  ${Else}
    !if "${MINIMUMWEBVIEW2VERSION}" != ""
      ${VersionCompare} "${MINIMUMWEBVIEW2VERSION}" "$4" $R0
      ${If} $R0 = 1
        update_webview:
          DetailPrint "$(installingWebview2)"
          ${If} ${RunningX64}
            ReadRegStr $R1 HKLM "SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate" "path"
          ${Else}
            ReadRegStr $R1 HKLM "SOFTWARE\Microsoft\EdgeUpdate" "path"
          ${EndIf}
          ${If} $R1 == ""
            ReadRegStr $R1 HKCU "SOFTWARE\Microsoft\EdgeUpdate" "path"
          ${EndIf}
          ${If} $R1 != ""
            ; Chromium updater docs: https://source.chromium.org/chromium/chromium/src/+/main:docs/updater/user_manual.md
            ; Modified from "HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\Microsoft EdgeWebView\ModifyPath"
            ExecWait `"$R1" /install appguid=${WEBVIEW2APPGUID}&needsadmin=true` $1
            ${If} $1 = 0
              DetailPrint "$(webview2InstallSuccess)"
            ${Else}
              MessageBox MB_ICONEXCLAMATION|MB_ABORTRETRYIGNORE "$(webview2InstallError)" IDIGNORE ignore IDRETRY update_webview
              Quit
              ignore:
            ${EndIf}
          ${EndIf}
      ${EndIf}
    !endif
  ${EndIf}
SectionEnd

Section Install
  SetOutPath $INSTDIR

  !ifmacrodef NSIS_HOOK_PREINSTALL
    !insertmacro NSIS_HOOK_PREINSTALL
  !endif

  !insertmacro CheckIfAppIsRunning "${MAINBINARYNAME}.exe" "${PRODUCTNAME}"

  ; Copy main executable
  File "${MAINBINARYSRCPATH}"

  ; Copy resources
  {{#each resources_dirs}}
    CreateDirectory "$INSTDIR\\{{this}}"
  {{/each}}
  {{#each resources}}
    File /a "/oname={{this.[1]}}" "{{no-escape @key}}"
  {{/each}}

  ; Copy external binaries
  {{#each binaries}}
    File /a "/oname={{this}}" "{{no-escape @key}}"
  {{/each}}

  ; Create file associations
  {{#each file_associations as |association| ~}}
    {{#each association.ext as |ext| ~}}
       !insertmacro APP_ASSOCIATE "{{ext}}" "{{or association.name ext}}" "{{association-description association.description ext}}" "$INSTDIR\${MAINBINARYNAME}.exe,0" "Open with ${PRODUCTNAME}" "$INSTDIR\${MAINBINARYNAME}.exe $\"%1$\""
    {{/each}}
  {{/each}}

  ; Register deep links
  {{#each deep_link_protocols as |protocol| ~}}
    WriteRegStr SHCTX "Software\Classes\\{{protocol}}" "URL Protocol" ""
    WriteRegStr SHCTX "Software\Classes\\{{protocol}}" "" "URL:${BUNDLEID} protocol"
    WriteRegStr SHCTX "Software\Classes\\{{protocol}}\DefaultIcon" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\",0"
    WriteRegStr SHCTX "Software\Classes\\{{protocol}}\shell\open\command" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\""
  {{/each}}

  ; Portable mode: skip every registry write + the uninstaller — leave only the
  ; copied files in the chosen folder. Standard mode does the full integration.
  ${If} $PortableMode != 1
  ; Create uninstaller
  WriteUninstaller "$INSTDIR\uninstall.exe"

  ; Save $INSTDIR in registry for future installations
  WriteRegStr SHCTX "${MANUPRODUCTKEY}" "" $INSTDIR

  !if "${INSTALLMODE}" == "both"
    ; Save install mode to be selected by default for the next installation such as updating
    ; or when uninstalling
    WriteRegStr SHCTX "${UNINSTKEY}" $MultiUser.InstallMode 1
  !endif

  ; Remove old main binary if it doesn't match new main binary name
  ReadRegStr $OldMainBinaryName SHCTX "${UNINSTKEY}" "MainBinaryName"
  ${If} $OldMainBinaryName != ""
  ${AndIf} $OldMainBinaryName != "${MAINBINARYNAME}.exe"
    Delete "$INSTDIR\$OldMainBinaryName"
  ${EndIf}

  ; Save current MAINBINARYNAME for future updates
  WriteRegStr SHCTX "${UNINSTKEY}" "MainBinaryName" "${MAINBINARYNAME}.exe"

  ; Registry information for add/remove programs
  WriteRegStr SHCTX "${UNINSTKEY}" "DisplayName" "${PRODUCTNAME}"
  WriteRegStr SHCTX "${UNINSTKEY}" "DisplayIcon" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\""
  WriteRegStr SHCTX "${UNINSTKEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr SHCTX "${UNINSTKEY}" "Publisher" "${MANUFACTURER}"
  WriteRegStr SHCTX "${UNINSTKEY}" "InstallLocation" "$\"$INSTDIR$\""
  WriteRegStr SHCTX "${UNINSTKEY}" "UninstallString" "$\"$INSTDIR\uninstall.exe$\""
  WriteRegDWORD SHCTX "${UNINSTKEY}" "NoModify" "1"
  WriteRegDWORD SHCTX "${UNINSTKEY}" "NoRepair" "1"

  ${GetSize} "$INSTDIR" "/M=uninstall.exe /S=0K /G=0" $0 $1 $2
  IntOp $0 $0 + ${ESTIMATEDSIZE}
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD SHCTX "${UNINSTKEY}" "EstimatedSize" "$0"

  !if "${HOMEPAGE}" != ""
    WriteRegStr SHCTX "${UNINSTKEY}" "URLInfoAbout" "${HOMEPAGE}"
    WriteRegStr SHCTX "${UNINSTKEY}" "URLUpdateInfo" "${HOMEPAGE}"
    WriteRegStr SHCTX "${UNINSTKEY}" "HelpLink" "${HOMEPAGE}"
  !endif

  ; Create start menu shortcut
  !insertmacro MUI_STARTMENU_WRITE_BEGIN Application
    Call CreateOrUpdateStartMenuShortcut
  !insertmacro MUI_STARTMENU_WRITE_END

  ; Create desktop shortcut for silent and passive installers
  ; because finish page will be skipped
  ${If} $PassiveMode = 1
  ${OrIf} ${Silent}
    Call CreateOrUpdateDesktopShortcut
  ${EndIf}
  ${EndIf}

  !ifmacrodef NSIS_HOOK_POSTINSTALL
    !insertmacro NSIS_HOOK_POSTINSTALL
  !endif

  ; Auto close this page for passive mode
  ${If} $PassiveMode = 1
    SetAutoClose true
  ${EndIf}
SectionEnd

Function .onInstSuccess
  ; Check for `/R` flag only in silent and passive installers because
  ; GUI installer has a toggle for the user to (re)start the app
  ${If} $PassiveMode = 1
  ${OrIf} ${Silent}
    ${GetOptions} $CMDLINE "/R" $R0
    ${IfNot} ${Errors}
      ${GetOptions} $CMDLINE "/ARGS" $R0
      nsis_tauri_utils::RunAsUser "$INSTDIR\${MAINBINARYNAME}.exe" "$R0"
    ${EndIf}
  ${EndIf}
FunctionEnd

Function un.onInit
  !insertmacro SetContext

  !if "${INSTALLMODE}" == "both"
    !insertmacro MULTIUSER_UNINIT
  !endif

  !insertmacro MUI_UNGETLANGUAGE

  ${GetOptions} $CMDLINE "/P" $PassiveMode
  ${IfNot} ${Errors}
    StrCpy $PassiveMode 1
  ${EndIf}

  ${GetOptions} $CMDLINE "/UPDATE" $UpdateMode
  ${IfNot} ${Errors}
    StrCpy $UpdateMode 1
  ${EndIf}
FunctionEnd

Section Uninstall

  !ifmacrodef NSIS_HOOK_PREUNINSTALL
    !insertmacro NSIS_HOOK_PREUNINSTALL
  !endif

  !insertmacro CheckIfAppIsRunning "${MAINBINARYNAME}.exe" "${PRODUCTNAME}"

  ; Delete the app directory and its content from disk
  ; Copy main executable
  Delete "$INSTDIR\${MAINBINARYNAME}.exe"

  ; Delete resources
  {{#each resources}}
    Delete "$INSTDIR\\{{this.[1]}}"
  {{/each}}

  ; Delete external binaries
  {{#each binaries}}
    Delete "$INSTDIR\\{{this}}"
  {{/each}}

  ; Delete app associations
  {{#each file_associations as |association| ~}}
    {{#each association.ext as |ext| ~}}
      !insertmacro APP_UNASSOCIATE "{{ext}}" "{{or association.name ext}}"
    {{/each}}
  {{/each}}

  ; Delete deep links
  {{#each deep_link_protocols as |protocol| ~}}
    ReadRegStr $R7 SHCTX "Software\Classes\\{{protocol}}\shell\open\command" ""
    ${If} $R7 == "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\""
      DeleteRegKey SHCTX "Software\Classes\\{{protocol}}"
    ${EndIf}
  {{/each}}


  ; Delete uninstaller
  Delete "$INSTDIR\uninstall.exe"

  {{#each resources_ancestors}}
  RMDir /REBOOTOK "$INSTDIR\\{{this}}"
  {{/each}}
  RMDir "$INSTDIR"

  ; Remove shortcuts if not updating
  ${If} $UpdateMode <> 1
    !insertmacro DeleteAppUserModelId

    ; Remove start menu shortcut
    !insertmacro MUI_STARTMENU_GETFOLDER Application $AppStartMenuFolder
    !insertmacro IsShortcutTarget "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    Pop $0
    ${If} $0 = 1
      !insertmacro UnpinShortcut "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk"
      Delete "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk"
      RMDir "$SMPROGRAMS\$AppStartMenuFolder"
    ${EndIf}
    !insertmacro IsShortcutTarget "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    Pop $0
    ${If} $0 = 1
      !insertmacro UnpinShortcut "$SMPROGRAMS\${PRODUCTNAME}.lnk"
      Delete "$SMPROGRAMS\${PRODUCTNAME}.lnk"
    ${EndIf}

    ; Remove desktop shortcuts
    !insertmacro IsShortcutTarget "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    Pop $0
    ${If} $0 = 1
      !insertmacro UnpinShortcut "$DESKTOP\${PRODUCTNAME}.lnk"
      Delete "$DESKTOP\${PRODUCTNAME}.lnk"
    ${EndIf}
  ${EndIf}

  ; Remove registry information for add/remove programs
  !if "${INSTALLMODE}" == "both"
    DeleteRegKey SHCTX "${UNINSTKEY}"
  !else if "${INSTALLMODE}" == "perMachine"
    DeleteRegKey HKLM "${UNINSTKEY}"
  !else
    DeleteRegKey HKCU "${UNINSTKEY}"
  !endif

  ; Removes the Autostart entry from the HKCU Run key if it exists.
  ; This ensures the program does not launch automatically after uninstallation if it exists.
  ; If it doesn't exist, it does nothing.
  ; We do this when not updating (to preserve the registry value on updates)
  ${If} $UpdateMode <> 1
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${AUTOSTARTKEY}"
  ${EndIf}

  ; Delete the data folder if that was chosen on the data page, and if not updating
  ${If} $DeleteDataState = 1
  ${AndIf} $UpdateMode <> 1
    ; Clear the install location $INSTDIR from registry
    DeleteRegKey SHCTX "${MANUPRODUCTKEY}"
    DeleteRegKey /ifempty SHCTX "${MANUKEY}"

    ; Clear the install language from registry
    DeleteRegValue HKCU "${MANUPRODUCTKEY}" "Installer Language"
    DeleteRegKey /ifempty HKCU "${MANUPRODUCTKEY}"
    DeleteRegKey /ifempty HKCU "${MANUKEY}"

    SetShellVarContext current
    ; Canonical store, the same ${DATAFOLDER} the data page just measured; legacy
    ; bundle-id dirs kept for older installs. A store relocated via the expert
    ; menu is not reachable, and only this named folder is ever removed, never
    ; its parent.
    RmDir /r "$APPDATA\${DATAFOLDER}"
    RmDir /r "$APPDATA\${BUNDLEID}"
    RmDir /r "$LOCALAPPDATA\${BUNDLEID}"
    ; Scratch files hold picture crops, filmstrips and working copies of the user's
    ; own files, so "delete my data" takes them along. Only the app's own folder
    ; and its own prefixed preview files are touched, never %TEMP% itself.
    RmDir /r "$TEMP\${TEMPFOLDER}"
    Delete "$TEMP\${TEMPPREFIX}-snip-*"
  ${EndIf}

  !ifmacrodef NSIS_HOOK_POSTUNINSTALL
    !insertmacro NSIS_HOOK_POSTUNINSTALL
  !endif

  ; Auto close if passive mode or updating
  ${If} $PassiveMode = 1
  ${OrIf} $UpdateMode = 1
    SetAutoClose true
  ${EndIf}
SectionEnd

Function RestorePreviousInstallLocation
  ReadRegStr $4 SHCTX "${MANUPRODUCTKEY}" ""
  StrCmp $4 "" +2 0
    StrCpy $INSTDIR $4
FunctionEnd

Function Skip
  Abort
FunctionEnd

Function SkipIfPassive
  ${IfThen} $PassiveMode = 1  ${|} Abort ${|}
FunctionEnd
Function un.SkipIfPassive
  ${IfThen} $PassiveMode = 1  ${|} Abort ${|}
FunctionEnd

Function CreateOrUpdateStartMenuShortcut
  ; We used to use product name as MAINBINARYNAME
  ; migrate old shortcuts to target the new MAINBINARYNAME
  StrCpy $R0 0

  !insertmacro IsShortcutTarget "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk" "$INSTDIR\$OldMainBinaryName"
  Pop $0
  ${If} $0 = 1
    !insertmacro SetShortcutTarget "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    StrCpy $R0 1
  ${EndIf}

  !insertmacro IsShortcutTarget "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$INSTDIR\$OldMainBinaryName"
  Pop $0
  ${If} $0 = 1
    !insertmacro SetShortcutTarget "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    StrCpy $R0 1
  ${EndIf}

  ${If} $R0 = 1
    Return
  ${EndIf}

  ; Skip creating shortcut if in update mode or no shortcut mode
  ; but always create if migrating from wix
  ${If} $WixMode = 0
    ${If} $UpdateMode = 1
    ${OrIf} $NoShortcutMode = 1
      Return
    ${EndIf}
  ${EndIf}

  !if "${STARTMENUFOLDER}" != ""
    CreateDirectory "$SMPROGRAMS\$AppStartMenuFolder"
    CreateShortcut "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    !insertmacro SetLnkAppUserModelId "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk"
  !else
    CreateShortcut "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    !insertmacro SetLnkAppUserModelId "$SMPROGRAMS\${PRODUCTNAME}.lnk"
  !endif
FunctionEnd

Function CreateOrUpdateDesktopShortcut
  ; We used to use product name as MAINBINARYNAME
  ; migrate old shortcuts to target the new MAINBINARYNAME
  !insertmacro IsShortcutTarget "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\$OldMainBinaryName"
  Pop $0
  ${If} $0 = 1
    !insertmacro SetShortcutTarget "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    Return
  ${EndIf}

  ; Skip creating shortcut if in update mode or no shortcut mode
  ; but always create if migrating from wix
  ${If} $WixMode = 0
    ${If} $UpdateMode = 1
    ${OrIf} $NoShortcutMode = 1
      Return
    ${EndIf}
  ${EndIf}

  CreateShortcut "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
  !insertmacro SetLnkAppUserModelId "$DESKTOP\${PRODUCTNAME}.lnk"
FunctionEnd
