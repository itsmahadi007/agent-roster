; Installer hooks for Windows (Tauri NSIS bundle).
;
; Every agent terminal runs through `roster.exe run`, which stays alive as long as the
; agent does, and Windows locks a running program file. Writing the new roster.exe over
; it fails ("Can't write ... roster.exe") and aborts the update. Windows does allow
; renaming a running program, so move the old copy aside first: running agents keep
; using it, and the new copy is written to the free name.

!macro NSIS_HOOK_PREINSTALL
  ; Copies moved aside by an earlier update. One still in use stays until the next update.
  Delete "$INSTDIR\roster-old-*.exe"
  IfFileExists "$INSTDIR\roster.exe" 0 roster_aside_done
  Push $R9
  StrCpy $R9 0
  roster_aside_next:
    IntOp $R9 $R9 + 1
    IfFileExists "$INSTDIR\roster-old-$R9.exe" roster_aside_next
  ClearErrors
  Rename "$INSTDIR\roster.exe" "$INSTDIR\roster-old-$R9.exe"
  Pop $R9
  roster_aside_done:
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  Delete "$INSTDIR\roster-old-*.exe"
!macroend
