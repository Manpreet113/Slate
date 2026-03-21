import QtQuick
import QtQuick.Layouts
import QtQuick.Effects
import Quickshell
import "."
import "modules" as Modules

Item {
    id: bar
    // Floating look: not full width
    implicitHeight: Config.barHeight
    implicitWidth: 800 
    
    // Background with Molecular Glass
    Rectangle {
        id: bg
        anchors.fill: parent
        color: Config.bg
        opacity: Config.bgOpacity
        radius: Config.radius
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

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: Config.padding * 2
        anchors.rightMargin: Config.padding * 2
        spacing: Config.padding
        
        // Left: Workspaces
        Item {
            Layout.fillWidth: true
            Layout.preferredHeight: parent.height
            Modules.WorkspaceTracker {
                anchors.verticalCenter: parent.verticalCenter
            }
            MouseArea {
                anchors.fill: parent
                onClicked: root.showLauncher = !root.showLauncher
            }
        }
        
        // Center: Clock
        Item {
            Layout.preferredHeight: parent.height
            Modules.Clock {
                anchors.centerIn: parent
            }
            MouseArea {
                anchors.fill: parent
                onClicked: root.showCommandCenter = !root.showCommandCenter
            }
        }
        
        // Right: Stats
        Item {
            Layout.fillWidth: true
            Layout.preferredHeight: parent.height
            Modules.SystemStats {
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
            }
            MouseArea {
                anchors.fill: parent
                onClicked: root.showCommandCenter = !root.showCommandCenter
            }
        }
    }
}
