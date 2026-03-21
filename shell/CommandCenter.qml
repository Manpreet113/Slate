import QtQuick
import QtQuick.Layouts
import QtQuick.Effects
import Quickshell
import Quickshell.Io
import "."

Item {
    id: commandCenter
    
    // Background
    Rectangle {
        id: bg
        anchors.fill: parent
        color: Config.bg
        opacity: Config.bgOpacity + 0.1
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

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: Config.padding * 2
        spacing: Config.padding * 2
        
        Text {
            text: "Command Center"
            color: Config.accent
            font.bold: true
            font.pixelSize: 18
        }
        
        // Toggles Grid
        GridLayout {
            columns: 2
            Layout.fillWidth: true
            rowSpacing: Config.padding
            columnSpacing: Config.padding
            
            Repeater {
                model: [
                    { name: "WiFi", icon: "󰖩", state: true },
                    { name: "Bluetooth", icon: "󰂯", state: false },
                    { name: "Dark Mode", icon: "󰖔", state: true },
                    { name: "DND", icon: "󰂛", state: false }
                ]
                
                delegate: Rectangle {
                    Layout.fillWidth: true
                    height: 60
                    radius: Config.radius / 2
                    color: modelData.state ? Config.accent : "#333333"
                    opacity: modelData.state ? 1.0 : 0.6
                    
                    RowLayout {
                        anchors.centerIn: parent
                        spacing: 8
                        Text { text: modelData.icon; color: "white"; font.pixelSize: 20 }
                        Text { text: modelData.name; color: "white"; font.bold: true }
                    }
                    
                    MouseArea {
                        anchors.fill: parent
                        onClicked: console.log("Toggled " + modelData.name)
                    }
                }
            }
        }
        
        // Real Sliders
        ColumnLayout {
            Layout.fillWidth: true
            spacing: Config.padding
            
            // Brightness
            Text { text: "Brightness"; color: "white"; opacity: 0.6 }
            Item {
                Layout.fillWidth: true; height: 30
                Rectangle { anchors.fill: parent; radius: 15; color: "#222222" }
                Rectangle { 
                    width: parent.width * (commandCenter.brightnessVal / 100)
                    height: parent.height; radius: 15; color: Config.accent 
                }
                MouseArea {
                    anchors.fill: parent
                    onClicked: (mouse) => {
                        let val = Math.round((mouse.x / width) * 100);
                        commandCenter.brightnessVal = val;
                        brightnessSetProc.run(["brightnessctl", "s", val + "%"]);
                    }
                }
            }
            
            // Volume
            Text { text: "Volume"; color: "white"; opacity: 0.6 }
            Item {
                Layout.fillWidth: true; height: 30
                Rectangle { anchors.fill: parent; radius: 15; color: "#222222" }
                Rectangle { 
                    width: parent.width * (commandCenter.volumeVal / 100)
                    height: parent.height; radius: 15; color: Config.accent 
                }
                MouseArea {
                    anchors.fill: parent
                    onClicked: (mouse) => {
                        let val = Math.round((mouse.x / width) * 100);
                        commandCenter.volumeVal = val;
                        volumeSetProc.run(["wpctl", "set-volume", "@DEFAULT_AUDIO_SINK@", val + "%"]);
                    }
                }
            }
        }
        
        // IPC Processes
        property int brightnessVal: 70
        property int volumeVal: 50
        
        Process { id: brightnessSetProc }
        Process { id: volumeSetProc }
        
        Process {
            id: brightnessGetProc
            command: ["brightnessctl", "g"]
            running: true
            stdout: SplitParser {
                onRead: (data) => {
                    let val = parseInt(data.trim());
                    if (!isNaN(val)) brightnessVal = Math.round((val / 255) * 100);
                }
            }
        }
        
        // Toggles IPC
        Process { id: wifiToggleProc; command: ["nmcli", "radio", "wifi"] }
        Process { id: btToggleProc; command: ["bluetoothctl", "power"] }
        
        Item { Layout.fillHeight: true } // Spacer
    }
}
