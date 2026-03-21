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
    
    // Background Overlay
    Rectangle {
        id: bg
        anchors.fill: parent
        color: "black"
        opacity: 0.4
    }
    
    MultiEffect {
        source: bg
        anchors.fill: bg
        blurEnabled: true
        blur: Config.blurRadius * 3
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 100
        spacing: 50
        
        // Search Bar
        Rectangle {
            Layout.alignment: Qt.AlignHCenter
            width: 600
            height: 60
            color: "#1A1A1A"
            radius: 30
            border.color: Config.borderColor
            border.width: 1
            
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 20
                anchors.rightMargin: 20
                Text { text: "󰍉"; color: "white"; opacity: 0.6; font.pixelSize: 24 }
                TextInput {
                    id: searchInput
                    Layout.fillWidth: true
                    color: "white"
                    font.pixelSize: 20
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
                    spacing: 15
                    
                    Rectangle {
                        Layout.alignment: Qt.AlignHCenter
                        width: 100
                        height: 100
                        radius: 20
                        color: modelData.color
                        
                        Text {
                            anchors.centerIn: parent
                            text: modelData.icon
                            color: "white"
                            font.bold: true
                            font.pixelSize: 40
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
                        color: "white"
                        font.bold: true
                        font.pixelSize: 16
                    }
                }
            }
        }
    }
    
    Process { id: launcherExec }
}
