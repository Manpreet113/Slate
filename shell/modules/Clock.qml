import QtQuick
import Quickshell
import ".."

Item {
    id: clockModule
    width: childrenRect.width
    height: childrenRect.height
    
    property var currentTime: new Date()
    
    Timer {
        interval: 1000
        repeat: true
        running: true
        onTriggered: currentTime = new Date()
    }
    
    Text {
        text: Qt.formatDateTime(currentTime, "HH:mm")
        color: Config.accent
        font.family: Config.sansFont
        font.pointSize: Config.fontSize + 2
        font.bold: true
    }
}
