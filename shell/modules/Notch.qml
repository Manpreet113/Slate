import QtQuick
import QtQuick.Layouts
import QtQuick.Effects
import Quickshell
import "../services" as Services
import ".."

Item {
    id: notch
    
    property bool expanded: false
    
    implicitWidth: expanded ? 400 : 180
    implicitHeight: 36
    
    Behavior on implicitWidth { NumberAnimation { duration: 300; easing.type: Easing.OutBack } }

    // Background (Molecular Glass)
    Rectangle {
        id: bg
        anchors.fill: parent
        color: Config.bg
        opacity: Config.bgOpacity + 0.1
        radius: height / 2
        border.color: Config.borderColor
        border.width: 1
        
        layer.enabled: true
        layer.effect: MultiEffect {
            blurEnabled: true
            blur: Config.blurRadius
            shadowEnabled: true
            shadowOpacity: Config.shadowOpacity
            shadowBlur: Config.shadowBlur
            shadowColor: "black"
        }
    }

    // Content
    RowLayout {
        anchors.centerIn: parent
        spacing: 15
        opacity: notch.expanded ? 0 : 1
        visible: opacity > 0
        Behavior on opacity { NumberAnimation { duration: 200 } }

        Text { text: Services.Network.connected ? "󰖩" : "󰖪"; color: Services.Network.connected ? Config.accent : "white"; font.pixelSize: 16 }
        Text { text: Services.Audio.muted ? "󰝟" : "󰕾"; color: "white"; font.pixelSize: 16 }
        Text { 
            text: Qt.formatDateTime(new Date(), "HH:mm")
            color: Config.textPrimary
            font.family: Config.mainFont
            font.bold: true
            font.pixelSize: 14 
        }
    }
    
    // Expanded Content (e.g., Notification or Title)
    Text {
        anchors.centerIn: parent
        text: "Elysium Shell"
        color: Config.textPrimary
        font.family: Config.mainFont
        font.bold: true
        font.pixelSize: 16
        opacity: notch.expanded ? 1 : 0
        visible: opacity > 0
        Behavior on opacity { NumberAnimation { duration: 200 } }
    }

    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        onEntered: notch.expanded = true
        onExited: notch.expanded = false
        onClicked: root.showDashboard = !root.showDashboard
    }
}
