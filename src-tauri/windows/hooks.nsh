; Installer hooks for the Windows setup program (referenced from tauri.conf.json).
;
; Pluto used to be called Loaf. A different name is a different program to Windows, so installing
; Pluto left the old Loaf installed and, if it was running, still running: two pets, two copies
; of the app, and a login entry for each. This removes the old one when Pluto is installed.
;
;   * It only does anything when an old Loaf is registered (nothing for a new install, and
;     nothing on an update from one Pluto to the next).
;   * A Loaf from the `-setup.exe` installer is removed by its own uninstaller, silently. A Loaf
;     from the `.msi` is removed through Windows Installer (`msiexec /x`), which shows its progress
;     and, for an all-users install, asks Windows for permission (the person can say no).
;   * Either way the person's data is kept: the setup uninstaller deletes the data folder only when
;     its "Delete app data" box is ticked (a silent run never ticks it), and an MSI removal never
;     touches it. Pluto copies that data across on its first start (`pluto_core::legacy_data`).
;   * The old program is stopped first so nothing is waiting on it.

; Find a Loaf installed from the .msi under one registry root. WiX files each product under its
; product code (a GUID in braces); the code is what `msiexec /x` needs. Sets $R3 to it, or "".
!macro _PLUTO_FIND_LOAF_MSI ROOT ID
  StrCpy $0 0
  loaf_msi_loop_${ID}:
    EnumRegKey $1 ${ROOT} "SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall" $0
    StrCmp $1 "" loaf_msi_done_${ID}
    IntOp $0 $0 + 1
    StrCpy $2 $1 1
    StrCmp $2 "{" 0 loaf_msi_loop_${ID}
    ReadRegStr $2 ${ROOT} "SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\$1" "DisplayName"
    StrCmp $2 "Loaf" 0 loaf_msi_loop_${ID}
    StrCpy $R3 $1
  loaf_msi_done_${ID}:
!macroend

!macro NSIS_HOOK_PREINSTALL
  Push $R0
  Push $R1
  Push $R2
  Push $R3
  Push $R4

  ; 1. A Loaf from the setup program.
  ReadRegStr $R0 SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\Loaf" "UninstallString"

  ; 2. A Loaf from the .msi (all-users first, then this user only).
  StrCpy $R3 ""
  StrCpy $R4 "machine"
  !insertmacro _PLUTO_FIND_LOAF_MSI HKLM machine
  ${If} $R3 == ""
    StrCpy $R4 "user"
    !insertmacro _PLUTO_FIND_LOAF_MSI HKCU user
  ${EndIf}

  ${If} $R0 != ""
  ${OrIf} $R3 != ""
    DetailPrint "Removing the earlier version (Loaf). Your notes, tasks and settings are kept."
    nsExec::Exec 'taskkill /F /IM loaf.exe'
    Pop $R2
    Sleep 800

    ${If} $R0 != ""
      ; Run the old uninstaller in place (`_?=`) so this waits for it, then tidy up what it
      ; cannot delete from itself: the same steps Tauri's own reinstall page takes.
      ReadRegStr $R1 SHCTX "${MANUKEY}\Loaf" ""
      ${If} $R1 != ""
        ExecWait '$R0 /S _?=$R1' $R2
        Delete "$R1\uninstall.exe"
        RMDir "$R1"
      ${Else}
        ExecWait '$R0 /S' $R2
      ${EndIf}
    ${EndIf}

    ${If} $R3 != ""
      ; `runas` asks Windows for permission, which an all-users install needs; a per-user one does not.
      ${If} $R4 == "machine"
        ExecShellWait "runas" "msiexec.exe" "/x $R3 /passive /norestart"
      ${Else}
        ExecShellWait "open" "msiexec.exe" "/x $R3 /passive /norestart"
      ${EndIf}
    ${EndIf}

    ; Its login entry, in case the uninstaller left it.
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Loaf"
  ${EndIf}

  Pop $R4
  Pop $R3
  Pop $R2
  Pop $R1
  Pop $R0
!macroend
