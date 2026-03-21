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
    
    // Pinned Apps (Top level to avoid scoping issues)
    readonly property var pinnedApps: [
        { name: "Terminal", class: "ghostty", icon: "G", color: "#333333" },
        { name: "Browser", class: "firefox", icon: "F", color: "#FF4500" },
        { name: "Code", class: "code-oss", icon: "C", color: "#007ACC" },
        { name: "Files", class: "nautilus", icon: "D", color: "#4CAF50" }
    ]

    // Background with Blur
    Rectangle {
        id: bg
        anchors.fill: parent
        color: Config.bg; opacity: Config.bgOpacity; radius: Config.radius
        border.color: Config.borderColor; border.width: 1
    }
    
    MultiEffect {
        source: bg; anchors.fill: bg
        blurEnabled: true; blur: Config.blurRadius
    }

    RowLayout {
        id: layout
        anchors.centerIn: parent
        spacing: Config.padding * 1.5; anchors.margins: Config.padding
        
        Repeater {
            model: dock.pinnedApps
            delegate: Item {
                width: 44; height: 44
                
                readonly property bool isRunning: {
                    if (!Hyprland.windows) return false;
                    for (var i = 0; i < Hyprland.windows.length; i++) {
                        let win = Hyprland.windows.get(i);
                        if (win && win.class.toLowerCase().includes(modelData.class)) return true;
                    }
                    return false;
                }
                
                readonly property bool isFocused: {
                    return Hyprland.focusedWindow && Hyprland.focusedWindow.class && Hyprland.focusedWindow.class.toLowerCase().includes(modelData.class)
                }

                Rectangle {
                    anchors.fill: parent; radius: 12
                    color: isFocused ? Config.accent : (isRunning ? "#444444" : "#222222")
                    opacity: isRunning ? 1.0 : 0.6
                    
                    Text {
                        anchors.centerIn: parent; text: modelData.icon
                        color: "white"; font.bold: true; font.pixelSize: 18
                    }
                    
                    Rectangle {
                        anchors.bottom: parent.bottom; anchors.bottomMargin: 4
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: 4; height: 4; radius: 2; color: "white"
                        visible: isRunning
                    }
                }
                
                MouseArea {
                    anchors.fill: parent; hoverEnabled: true
                    onEntered: parent.scale = 1.2; onExited: parent.scale = 1.0
                    onClicked: {
                        if (isRunning) Hyprland.dispatch("focuswindow class:" + modelData.class)
                    }
                }
                Behavior on scale { NumberAnimation { duration: 200; easing.type: Easing.OutBack } }
            }
        }
        
        Rectangle {
            visible: unpinnedRepeater.count > 0
            width: 1; height: 30; color: "white"; opacity: 0.1
        }
        
        Repeater {
            id: unpinnedRepeater
            model: Hyprland.windows || []
            delegate: Item {
                readonly property string winClass: modelData.class ? modelData.class.toLowerCase() : ""
                readonly property bool isPinned: {
                    for (var i = 0; i < dock.pinnedApps.length; i++) {
                        if (winClass.includes(dock.pinnedApps[i].class)) return true;
                    }
                    return false;
                }
                
                visible: !isPinned && (() => {
                    if (!Hyprland.windows) return false;
                    for (var j = 0; j < index; j++) {
                        let win = Hyprland.windows.get(j);
                        if (win && win.class === modelData.class) return false;
                    }
                    return true;
                })()
                
                width: visible ? 44 : 0; height: 44; clip: true
                
                Rectangle {
                    anchors.fill: parent; radius: 12
                    color: modelData.focus ? Config.accent : "#333333"
                    Text {
                        anchors.centerIn: parent; text: winClass.substring(0, 1).toUpperCase()
                        color: "white"; font.bold: true; font.pixelSize: 18
                    }
                }
                
                MouseArea {
                    anchors.fill: parent
                    onClicked: Hyprland.dispatch("focuswindow address:" + modelData.address)
                }
            }
        }
    }
}
