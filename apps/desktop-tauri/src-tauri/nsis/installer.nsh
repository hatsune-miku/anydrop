; AnyDrop installer hooks.
;
; The post-install hook adds a Windows Defender Firewall rule so AnyDrop can
; accept inbound LAN connections on any port it chooses to listen on. This
; covers all three of our current listeners (UDP 9818 discovery, UDP 9819
; QUIC transfer, TCP 9819 text broadcast) plus the mDNS responder, AND any
; future port re-configuration the user does from inside the app.
;
; Without this, on Windows Server (and on Win10/11 under the "Public"
; network profile), the OS firewall silently drops inbound LAN packets
; addressed to us — peers can't discover us, and our QUIC transfer server
; refuses every incoming connection. The symptom users see is "device A
; can find device B but not vice versa" + "file transfer hangs". Diagnosed
; in the field on a Windows Server 2022 box (Xiaomi BE6500 Pro mesh, no
; router-side AP isolation involved).
;
; Scope: by program path, not by port. More resilient — if the user changes
; the discovery / data port in settings, the rule still applies.

!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Adding Windows Defender Firewall rule for AnyDrop..."
  ; Delete any leftover rule with the same name first, so re-installs don't
  ; pile up duplicates and `add rule` doesn't fail.
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="AnyDrop"'
  nsExec::ExecToLog 'netsh advfirewall firewall add rule name="AnyDrop" dir=in action=allow program="$INSTDIR\AnyDrop.exe" profile=any enable=yes description="LAN discovery and file transfer (added by AnyDrop installer)"'
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  DetailPrint "Removing AnyDrop Windows Defender Firewall rule..."
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="AnyDrop"'
!macroend
