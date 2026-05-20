; AnyDrop NSIS installer hooks
;
; Adds Windows firewall rules so inbound LAN discovery (UDP 9818) and data
; transfer (TCP 9819) are allowed without prompting the user. Required for
; cross-platform sends where the remote peer initiates the TCP connection
; toward Windows.

!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Adding Windows Firewall rules for AnyDrop..."

  ; TCP data transfer (incoming)
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="AnyDrop TCP"'
  nsExec::ExecToLog 'netsh advfirewall firewall add rule name="AnyDrop TCP" dir=in action=allow protocol=TCP localport=9819 profile=any'

  ; UDP discovery (incoming)
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="AnyDrop UDP"'
  nsExec::ExecToLog 'netsh advfirewall firewall add rule name="AnyDrop UDP" dir=in action=allow protocol=UDP localport=9818 profile=any'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DetailPrint "Removing AnyDrop firewall rules..."
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="AnyDrop TCP"'
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="AnyDrop UDP"'
!macroend
