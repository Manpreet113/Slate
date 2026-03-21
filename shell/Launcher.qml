import QtQuick
import QtQuick.Layouts
import QtQuick.Effects
import Quickshell
import Quickshell.Io
import "."

Item {
    id: launcher
    
    // Search State
    property string searchText: ""
    
    // Background Overlay with High Blur
    Rectangle {
        id: bg
        anchors.fill: parent
        color: "black"
        opacity: 0.5
        
        layer.enabled: true
        layer.effect: MultiEffect {
            blurEnabled: true
            blur: Config.blurRadius * 3
        }

        MouseArea {
            anchors.fill: parent
            onClicked: root.showLauncher = false
        }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 100
        spacing: 60
        
        // Search Bar (Molecular Glass)
        Rectangle {
            Layout.alignment: Qt.AlignHCenter
            width: 650
            height: 64
            color: Qt.rgba(255, 255, 255, 0.05)
            radius: 32
            border.color: Config.borderColor
            border.width: 1
            
            layer.enabled: true
            layer.effect: MultiEffect {
                shadowEnabled: true
                shadowOpacity: 0.2
                shadowBlur: 0.5
                shadowColor: "black"
            }
            
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 24
                anchors.rightMargin: 24
                spacing: 15
                Text { text: "󰍉"; color: Config.accent; font.pixelSize: 24 }
                TextInput {
                    id: searchInput
                    Layout.fillWidth: true
                    color: "white"
                    font.family: Config.mainFont
                    font.pixelSize: 22
                    focus: true
                    onTextChanged: launcher.searchText = text
                }
            }
        }
        
        // App Grid
        GridView {
            id: appGrid
            Layout.fillWidth: true
            Layout.fillHeight: true
            cellWidth: 160
            cellHeight: 180
            clip: true
            
            model: {
                let apps = [
                    { name: "Firefox", icon: "F", color: "#FF4500", exec: "firefox" },
                    { name: "Ghostty", icon: "G", color: "#333333", exec: "ghostty" },
                    { name: "VS Code", icon: "C", color: "#007ACC", exec: "code" },
                    { name: "Files", icon: "D", color: "#4CAF50", exec: "nautilus" },
                    { name: "Spotify", icon: "S", color: "#1DB954", exec: "spotify" },
                    { name: "Discord", icon: "D", color: "#5865F2", exec: "discord" }
                ];
                if (launcher.searchText === "") return apps;
                return apps.filter(app => app.name.toLowerCase().includes(launcher.searchText.toLowerCase()));
            }
            
            delegate: Item {
                width: 160
                height: 180
                
                ColumnLayout {
                    anchors.centerIn: parent
                    spacing: 16
                    
                    Rectangle {
                        Layout.alignment: Qt.AlignHCenter
                        width: 100
                        height: 100
                        radius: 24
                        color: Qt.rgba(255, 255, 255, 0.05)
                        border.color: Config.borderColor
                        border.width: 1
                        
                        Rectangle {
                            anchors.fill: parent; anchors.margins: 10
                            radius: 18; color: modelData.color
                            Text {
                                anchors.centerIn: parent
                                text: modelData.icon
                                color: "white"; font.bold: true; font.pixelSize: 36
                            }
                        }
                        
                        MouseArea {
                            anchors.fill: parent
                            hoverEnabled: true
                            onEntered: parent.scale = 1.1
                            onExited: parent.scale = 1.0
                            onClicked: {
                                launcherExec.run(["sh", "-c", modelData.exec]);
                                root.showLauncher = false;
                            }
                        }
                        Behavior on scale { NumberAnimation { duration: 200; easing.type: Easing.OutBack } }
                    }
                    
                    Text {
                        Layout.alignment: Qt.AlignHCenter
                        text: modelData.name
                        color: Config.textPrimary
                        font.family: Config.mainFont
                        font.bold: true
                        font.pixelSize: 15
                    }
                }
            }
        }
    }
    
    Process { id: launcherExec }
}
