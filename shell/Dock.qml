import QtQuick
import QtQuick.Layouts
import QtQuick.Effects
import Quickshell
import "."

Item {
    id: dock
    implicitHeight: Config.dockHeight
    implicitWidth: childrenRect.width + Config.padding * 4
    
    // Background with Blur
    Rectangle {
        id: bg
        anchors.fill: parent
        color: Config.bg
        opacity: Config.bgOpacity
        radius: Config.radius
        border.color: Config.borderColor
        border.width: 1
        
        // Subtle outer glow
        layer.enabled: true
        layer.effect: MultiEffect {
            blurEnabled: true
            blur: 8
            opacity: 0.3
        }
    }
    
    MultiEffect {
        source: bg
        anchors.fill: bg
        blurEnabled: true
        blur: Config.blurRadius
    }

    RowLayout {
        anchors.centerIn: parent
        spacing: Config.padding * 1.5
        anchors.margins: Config.padding
        
        Repeater {
            model: [
                { name: "F", color: "#FF4500", label: "Firefox" },
                { name: "G", color: "#333333", label: "Ghostty" },
                { name: "C", color: "#007ACC", label: "Code" },
                { name: "F", color: "#4CAF50", label: "Files" },
                { name: "S", color: "#1DB954", label: "Spotify" }
            ]
            delegate: Item {
                width: 44
                height: 44
                
                Rectangle {
                    anchors.fill: parent
                    radius: 12
                    gradient: Gradient {
                        GradientStop { position: 0.0; color: modelData.color }
                        GradientStop { position: 1.0; color: Qt.darker(modelData.color, 1.2) }
                    }
                    
                    Text {
                        anchors.centerIn: parent
                        text: modelData.name
                        color: "white"
                        font.bold: true
                        font.pixelSize: 18
                    }
                }
                
                MouseArea {
                    anchors.fill: parent
                    hoverEnabled: true
                    onEntered: parent.scale = 1.2
                    onExited: parent.scale = 1.0
                }
                
                Behavior on scale { NumberAnimation { duration: 200; easing.type: Easing.OutBack } }
            }
        }
    }
}
