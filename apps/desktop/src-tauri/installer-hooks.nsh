; Advertise image formats in Open with. Leave each existing default app intact.
!macro FOTOEDITOR_ASSOCIATE EXT DESCRIPTION
  WriteRegStr SHCTX "Software\Classes\.${EXT}\OpenWithProgids" "${BUNDLEID}.${EXT}" ""
  WriteRegStr SHCTX "Software\Classes\${BUNDLEID}.${EXT}" "" "${DESCRIPTION}"
  WriteRegStr SHCTX "Software\Classes\${BUNDLEID}.${EXT}\DefaultIcon" "" '"$INSTDIR\${MAINBINARYNAME}.exe",0'
  WriteRegStr SHCTX "Software\Classes\${BUNDLEID}.${EXT}\shell\open\command" "" '"$INSTDIR\${MAINBINARYNAME}.exe" "%1"'
!macroend

!macro FOTOEDITOR_UNASSOCIATE EXT
  ReadRegStr $0 SHCTX "Software\Classes\.${EXT}" ""
  ${If} $0 == "${BUNDLEID}.${EXT}"
    DeleteRegValue SHCTX "Software\Classes\.${EXT}" ""
  ${EndIf}
  DeleteRegValue SHCTX "Software\Classes\.${EXT}\OpenWithProgids" "${BUNDLEID}.${EXT}"
  DeleteRegKey /ifempty SHCTX "Software\Classes\.${EXT}\OpenWithProgids"
  DeleteRegKey /ifempty SHCTX "Software\Classes\.${EXT}"
  DeleteRegKey SHCTX "Software\Classes\${BUNDLEID}.${EXT}"
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro FOTOEDITOR_ASSOCIATE "cgfoto" "CashGear FotoEditor project"
  !insertmacro FOTOEDITOR_ASSOCIATE "png" "PNG image"
  !insertmacro FOTOEDITOR_ASSOCIATE "jpg" "JPEG image"
  !insertmacro FOTOEDITOR_ASSOCIATE "jpeg" "JPEG image"
  ReadRegStr $0 SHCTX "Software\Classes\.cgfoto" ""
  ${If} $0 == ""
    WriteRegStr SHCTX "Software\Classes\.cgfoto" "" "${BUNDLEID}.cgfoto"
  ${EndIf}
  !insertmacro UPDATEFILEASSOC
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  !insertmacro FOTOEDITOR_UNASSOCIATE "cgfoto"
  !insertmacro FOTOEDITOR_UNASSOCIATE "png"
  !insertmacro FOTOEDITOR_UNASSOCIATE "jpg"
  !insertmacro FOTOEDITOR_UNASSOCIATE "jpeg"
  !insertmacro UPDATEFILEASSOC
!macroend
