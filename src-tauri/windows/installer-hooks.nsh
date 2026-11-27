; Troupe shipped as "Termul Manager" up to 0.4.60. Tauri's NSIS template keys
; the install folder and the Apps & features entry on PRODUCTNAME, so without
; these hooks installing or auto-updating to Troupe leaves Termul Manager
; installed next to it (tauri-apps/tauri#14175).
;
; PREINSTALL runs the old uninstaller silently and waits for it. App data stays:
; it lives under the unchanged identifier (com.termul-manager.app), and the
; uninstaller only deletes it when its "delete app data" checkbox is ticked,
; which a silent run never shows.
;
; PREUNINSTALL protects the shared agent homes (issue #2156). Account
; directories under the identifier folder link shared entries back to
; ~/.claude, ~/.codex and ~/.gemini; on Windows a shared directory is a
; junction. The template deletes app data with `RmDir /r`, which walks into a
; junction and deletes the target's contents. So before that, every directory
; junction or symlink in app data is unlinked, without following it. Troupe
; recreates the account links on startup (ensure_links).

!include LogicLib.nsh

!define TROUPE_LEGACY_PRODUCTNAME "Termul Manager"
!define TROUPE_LEGACY_UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${TROUPE_LEGACY_PRODUCTNAME}"

Var TroupeLegacyDir
Var TroupeLegacyBinary
Var TroupeLegacyStartMenu
Var TroupeLegacyDesktop

!macro NSIS_HOOK_PREINSTALL
  StrCpy $TroupeLegacyStartMenu 0
  StrCpy $TroupeLegacyDesktop 0

  ; Released builds install per user (HKCU); a per-machine install lands in HKLM.
  ReadRegStr $TroupeLegacyDir HKCU "${TROUPE_LEGACY_UNINSTKEY}" "InstallLocation"
  ReadRegStr $TroupeLegacyBinary HKCU "${TROUPE_LEGACY_UNINSTKEY}" "MainBinaryName"
  ${If} $TroupeLegacyDir == ""
    ReadRegStr $TroupeLegacyDir HKLM "${TROUPE_LEGACY_UNINSTKEY}" "InstallLocation"
    ReadRegStr $TroupeLegacyBinary HKLM "${TROUPE_LEGACY_UNINSTKEY}" "MainBinaryName"
  ${EndIf}

  ; The template writes InstallLocation wrapped in quotes.
  StrCpy $0 $TroupeLegacyDir 1
  ${If} $0 == '"'
    StrCpy $TroupeLegacyDir $TroupeLegacyDir "" 1
    StrCpy $TroupeLegacyDir $TroupeLegacyDir -1
  ${EndIf}
  ${If} $TroupeLegacyBinary == ""
    StrCpy $TroupeLegacyBinary "termul-manager.exe"
  ${EndIf}

  ${If} $TroupeLegacyDir != ""
  ${AndIf} ${FileExists} "$TroupeLegacyDir\uninstall.exe"
    ; The uninstaller deletes these; POSTINSTALL recreates them for Troupe,
    ; because an update-mode install never creates shortcuts itself.
    ${If} ${FileExists} "$SMPROGRAMS\${TROUPE_LEGACY_PRODUCTNAME}.lnk"
      StrCpy $TroupeLegacyStartMenu 1
    ${EndIf}
    ${If} ${FileExists} "$DESKTOP\${TROUPE_LEGACY_PRODUCTNAME}.lnk"
      StrCpy $TroupeLegacyDesktop 1
    ${EndIf}

    ; Asks before closing a running Termul Manager, or closes it in passive and
    ; silent mode, so the uninstaller below does not kill it unannounced.
    !insertmacro CheckIfAppIsRunning "$TroupeLegacyDir\$TroupeLegacyBinary" "${TROUPE_LEGACY_PRODUCTNAME}"

    DetailPrint "Removing ${TROUPE_LEGACY_PRODUCTNAME} from $TroupeLegacyDir"
    ; _?= runs the uninstaller in place instead of from a temp copy, so ExecWait
    ; really waits. It cannot delete itself while running, so that is done here.
    ClearErrors
    ExecWait '"$TroupeLegacyDir\uninstall.exe" /S _?=$TroupeLegacyDir' $0
    ${If} ${Errors}
    ${OrIf} $0 <> 0
      DetailPrint "${TROUPE_LEGACY_PRODUCTNAME} uninstaller failed (exit code $0); it stays installed"
    ${Else}
      Delete "$TroupeLegacyDir\uninstall.exe"
      RMDir "$TroupeLegacyDir"
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ${If} $TroupeLegacyStartMenu = 1
  ${AndIfNot} ${FileExists} "$SMPROGRAMS\${PRODUCTNAME}.lnk"
    CreateShortcut "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    !insertmacro SetLnkAppUserModelId "$SMPROGRAMS\${PRODUCTNAME}.lnk"
  ${EndIf}
  ${If} $TroupeLegacyDesktop = 1
  ${AndIfNot} ${FileExists} "$DESKTOP\${PRODUCTNAME}.lnk"
    CreateShortcut "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    !insertmacro SetLnkAppUserModelId "$DESKTOP\${PRODUCTNAME}.lnk"
  ${EndIf}
!macroend

; Set to 1 when a directory link in app data could not be removed.
Var TroupeLinkLeft

; Unlinks every directory junction or symlink below the directory on the stack.
; Never enters a reparse point: RMDir without /r removes only the link.
Function un.TroupeUnlinkDirLinks
  Exch $R0
  Push $R1
  Push $R2
  Push $R3
  Push $R4

  FindFirst $R1 $R2 "$R0\*"
  ${DoWhile} $R2 != ""
    ${If} $R2 != "."
    ${AndIf} $R2 != ".."
      StrCpy $R3 "$R0\$R2"
      ; Only directories (and links to them) match "\*.*"; files are left to
      ; the template, and DeleteFile on a file symlink removes only the link.
      ${If} ${FileExists} "$R3\*.*"
        System::Call 'kernel32::GetFileAttributesW(w R3) i .R4'
        ${If} $R4 <> -1
          IntOp $R4 $R4 & 0x400 ; FILE_ATTRIBUTE_REPARSE_POINT
          ${If} $R4 <> 0
            DetailPrint "Unlinking $R3"
            RMDir "$R3"
            System::Call 'kernel32::GetFileAttributesW(w R3) i .R4'
            ${If} $R4 <> -1
              StrCpy $TroupeLinkLeft 1
            ${EndIf}
          ${Else}
            Push $R3
            Call un.TroupeUnlinkDirLinks
          ${EndIf}
        ${EndIf}
      ${EndIf}
    ${EndIf}
    FindNext $R1 $R2
  ${Loop}
  FindClose $R1

  Pop $R4
  Pop $R3
  Pop $R2
  Pop $R1
  Pop $R0
FunctionEnd

; Unlinks the directory links in an app data root (on the stack). A root that
; is itself a link is unlinked and never entered, so RmDir /r has nothing to
; follow there.
Function un.TroupeUnlinkAppData
  Exch $R0
  Push $R1

  System::Call 'kernel32::GetFileAttributesW(w R0) i .R1'
  ${If} $R1 <> -1
    IntOp $R1 $R1 & 0x400 ; FILE_ATTRIBUTE_REPARSE_POINT
    ${If} $R1 <> 0
      DetailPrint "Unlinking $R0"
      RMDir "$R0"
      System::Call 'kernel32::GetFileAttributesW(w R0) i .R1'
      ${If} $R1 <> -1
        StrCpy $TroupeLinkLeft 1
      ${EndIf}
    ${Else}
      Push $R0
      Call un.TroupeUnlinkDirLinks
    ${EndIf}
  ${EndIf}

  Pop $R1
  Pop $R0
FunctionEnd

!macro NSIS_HOOK_PREUNINSTALL
  ; Same condition as the template's app data deletion: an unticked box, an
  ; update, or a silent run leave app data and its links alone.
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    ; The template checks for a running Troupe only after this hook. Check
    ; first: a running Troupe can recreate the links (ensure_links) after the
    ; sweep, and cancelling the close prompt must abort before any link is
    ; removed. The template's own check then finds nothing running.
    !insertmacro CheckIfAppIsRunning "$INSTDIR\${MAINBINARYNAME}.exe" "${PRODUCTNAME}"

    StrCpy $TroupeLinkLeft 0
    ${If} ${FileExists} "$APPDATA\${BUNDLEID}\*.*"
      Push "$APPDATA\${BUNDLEID}"
      Call un.TroupeUnlinkAppData
    ${EndIf}
    ${If} ${FileExists} "$LOCALAPPDATA\${BUNDLEID}\*.*"
      Push "$LOCALAPPDATA\${BUNDLEID}"
      Call un.TroupeUnlinkAppData
    ${EndIf}
    ; A link that is still there would be followed by RmDir /r. Keeping all
    ; app data is safer than deleting through it.
    ${If} $TroupeLinkLeft = 1
      DetailPrint "A linked folder in the app data could not be unlinked; app data is kept"
      StrCpy $DeleteAppDataCheckboxState 0
    ${EndIf}
  ${EndIf}
!macroend
