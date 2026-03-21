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
        font.family: Config.mainFont
        font.pointSize: 14
        font.bold: true
    }
}
