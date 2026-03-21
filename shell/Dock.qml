import QtQuick
import QtQuick.Layouts
import QtQuick.Effects
import Quickshell
import Quickshell.Hyprland
import "."

Item {
    id: dock
    implicitHeight: Config.dockHeight
    implicitWidth: layout.width + Config.padding * 4
    
    property bool shown: true // We'll link this to autohide logic in shell.qml
    
    // Background with Blur
    Rectangle {
        id: bg
        anchors.fill: parent
        color: Config.bg
        opacity: Config.bgOpacity
        radius: Config.radius
        border.color: Config.borderColor
        border.width: 1
    }
    
    MultiEffect {
        source: bg
        anchors.fill: bg
        blurEnabled: true
        blur: Config.blurRadius
    }

    RowLayout {
        id: layout
        anchors.centerIn: parent
        spacing: Config.padding * 1.5
        anchors.margins: Config.padding
        
        // Dynamic Taskbar: Group windows by class
        Repeater {
            model: Hyprland.windows
            
            delegate: Item {
                // Only show if it's the first window of this class in the list (simple grouping)
                // Note: This is an O(N^2) check in QML, but for ~10 windows it's fine.
                visible: {
                    for (var i = 0; i < index; i++) {
                        if (Hyprland.windows.get(i).class === modelData.class) return false;
                    }
                    return true;
                }
                
                width: visible ? 44 : 0
                height: 44
                clip: true
                
                Rectangle {
                    anchors.fill: parent
                    radius: 12
                    color: modelData.focus ? Config.accent : "#333333"
                    
                    // Animated underline for running apps
                    Rectangle {
                        anchors.bottom: parent.bottom
                        anchors.bottomMargin: 2
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: 12
                        height: 2
                        radius: 1
                        color: "white"
                        visible: true
                    }
                    
                    Text {
                        anchors.centerIn: parent
                        text: modelData.class.substring(0, 1).toUpperCase()
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
                    onClicked: Hyprland.dispatch("focuswindow address:" + modelData.address)
                }
                
                Behavior on scale { NumberAnimation { duration: 200; easing.type: Easing.OutBack } }
                Behavior on width { NumberAnimation { duration: 200 } }
            }
        }
    }
}
