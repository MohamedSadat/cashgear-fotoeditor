# P10 Windows desktop

P10 started on main at the verified `p9-retouch` commit. The desktop runs the
same editor, Rust WASM and engine worker as the browser build. The user chose a
free desktop app, an Authenticode-unsigned NSIS installer, the existing in-app
menu and one stable update channel on GitHub Releases.

## Implemented

- Windows uses native open and save dialogs, including image export. Explorer
  can open .cgfoto projects and offers FotoEditor under Open with for PNG and
  JPEG. The app handles drag and drop, remembers recently opened and saved
  projects, and restores window state. Subsequent Explorer launches route to
  the existing window and ask before replacing the current document.
- Native project I/O reads bounded ranges and streams writes to a temporary
  sibling file. The desktop host flushes that file before atomically replacing
  the destination. Cancellation and failed writes preserve the original.
- Updates verifies both the stable manifest and the version-bound installer
  signature. It checkpoints the current document in recovery before restarting.
  The release workflow assembles and signs the GitHub Release assets.
- Installer CI prepares a pair of locally signed packages and tests Explorer
  activation, editing, saving, tamper rejection, upgrading, recovery and
  uninstall. The same package pair can be verified in separate Windows guests.

## Measurements and verification

The local Windows x64 production WASM measured 4,547,606 bytes raw and
1,682,137 bytes at gzip level 9, below the 2,000,000-byte gate. A native project
probe opened and saved a filename with spaces and a non-ASCII character. The
three-layer input was 787,493 bytes and the file after duplicating a layer
was 893,895 bytes. Both WebView2 157.0.4341.60 backends passed on the development
workstation, with NVIDIA WebGPU and ANGLE D3D11 on a GeForce GTX 1650 for WebGL2.

On the Windows 10 Pro x64 development workstation (build 19045), the packaged
upgrade from 0.90.0 to 0.90.1 passed Explorer open, edit/save, single-instance
forwarding, both signature rejection tests, session recovery and uninstall.
One run with the 787,493-byte project measured 2.45 s to install, 3.76 s from
Explorer launch to opening the project, 0.82 s to save, 3.82 s from requesting
the verified update to the new process, and 1.44 s to uninstall and remove
the associations. The test update installer was 2,871,644 bytes. This uses the
workstation's installed WebView2 Runtime and a loopback update feed.

P10 completion requires a passing installer lifecycle on clean Windows 10 and
Windows 11 and successful CI on the tagged commit. Check the platform reports
and exact-head run recorded in the final phase evidence before certifying
`p10-desktop`. The stable Windows release is gated on that certification.

## Deferred work and phase boundary

The existing all-visible Clone Stamp speed deferral remains in P12. P11 is
the proposed next phase and covers 16-bit processing with enforced memory
budgets.
