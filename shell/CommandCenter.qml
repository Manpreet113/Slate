import QtQuick
import QtQuick.Layouts
import QtQuick.Effects
import Quickshell
import Quickshell.Io
import "."

Item {
    id: commandCenter
    
    // Background with Molecular Glass
    Rectangle {
        id: bg
        anchors.fill: parent
        color: Config.bg
        opacity: Config.bgOpacity + 0.1
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

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: Config.padding * 2.5
        spacing: Config.padding * 2
        
        // Header
        RowLayout {
            Layout.fillWidth: true
            Text {
                text: "Control Center"
                color: Config.textPrimary
                font.family: Config.mainFont
                font.bold: true
                font.pixelSize: 18
            }
            Item { Layout.fillWidth: true }
            Text {
                text: "󰒓"
                color: Config.textSecondary
                font.pixelSize: 18
            }
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
                    { name: "Night", icon: "󰖔", state: true },
                    { name: "DND", icon: "󰂛", state: false }
                ]
                
                delegate: Rectangle {
                    Layout.fillWidth: true
                    height: 56
                    radius: 12
                    color: modelData.state ? Config.accent : Qt.rgba(255, 255, 255, 0.05)
                    opacity: modelData.state ? 1.0 : 0.8
                    
                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: 15
                        spacing: 12
                        Text { text: modelData.icon; color: modelData.state ? "black" : "white"; font.pixelSize: 20 }
                        Text { 
                            text: modelData.name
                            color: modelData.state ? "black" : Config.textPrimary
                            font.family: Config.mainFont
                            font.bold: true 
                        }
                    }
                    
                    MouseArea {
                        anchors.fill: parent
                        onClicked: console.log("Toggled " + modelData.name)
                    }
                }
            }
        }
        
        // Sliders
        ColumnLayout {
            Layout.fillWidth: true
            spacing: Config.padding * 1.5
            
            // Brightness
            ColumnLayout {
                spacing: 6
                Text { text: "Brightness"; color: Config.textSecondary; font.pixelSize: 12 }
                Item {
                    Layout.fillWidth: true; height: 32
                    Rectangle { anchors.fill: parent; radius: 16; color: Qt.rgba(255, 255, 255, 0.05) }
                    Rectangle { 
                        width: parent.width * (commandCenter.brightnessVal / 100)
                        height: parent.height; radius: 16; color: Config.accent 
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
            }
            
            // Volume
            ColumnLayout {
                spacing: 6
                Text { text: "Volume"; color: Config.textSecondary; font.pixelSize: 12 }
                Item {
                    Layout.fillWidth: true; height: 32
                    Rectangle { anchors.fill: parent; radius: 16; color: Qt.rgba(255, 255, 255, 0.05) }
                    Rectangle { 
                        width: parent.width * (commandCenter.volumeVal / 100)
                        height: parent.height; radius: 16; color: Config.accent 
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
        }
        
        Item { Layout.fillHeight: true } // Spacer
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
}
