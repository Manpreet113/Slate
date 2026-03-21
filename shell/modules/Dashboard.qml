import QtQuick
import QtQuick.Layouts
import QtQuick.Effects
import Quickshell.Services.Mpris
import "../services" as Services
import "../components" as Components
import ".."

Rectangle {
    id: dashboard
    
    color: Config.bg
    opacity: Config.bgOpacity
    radius: Config.radiusLarge
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

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 25
        spacing: 20

        // Header: Profile & Status
        RowLayout {
            Layout.fillWidth: true
            spacing: 15
            
            Rectangle {
                width: 50; height: 50; radius: 25
                color: Config.accent
                Text { anchors.centerIn: parent; text: "E"; color: "white"; font.bold: true; font.pixelSize: 20 }
            }
            
            Column {
                Text { text: "Elysium User"; color: Config.textPrimary; font.family: Config.mainFont; font.pixelSize: 18; font.bold: true }
                Text { text: "System Active"; color: Config.textSecondary; font.family: Config.mainFont; font.pixelSize: 12 }
            }
        }

        // Toggles Grid
        GridLayout {
            columns: 2
            Layout.fillWidth: true
            rowSpacing: 10; columnSpacing: 10
            
            Components.DashboardToggle { 
                icon: Services.Network.connected ? "󰖩" : "󰖪"
                label: Services.Network.connected ? Services.Network.ssid : "WiFi Off"
                active: Services.Network.connected
                onClicked: Services.Network.toggle()
            }
            Components.DashboardToggle { 
                icon: "󰂯"
                label: "Bluetooth"
                active: false // To be implemented with BT service
            }
        }

        // Sliders
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 15
            
            Components.SliderRow {
                id: volSlider
                icon: Services.Audio.muted ? "󰝟" : "󰕾"
                value: Services.Audio.volume
                onMoved: (val) => Services.Audio.setVolume(val)
            }
            
            Components.SliderRow {
                id: brightSlider
                icon: "󰃟"
                value: Services.Brightness.value
                onMoved: (val) => Services.Brightness.setValue(val)
            }
        }

        // Media Player
        Rectangle {
            id: mediaBox
            Layout.fillWidth: true
            Layout.preferredHeight: 120
            color: "white"
            opacity: 0.05
            radius: Config.radiusMedium
            border.color: Config.borderColor
            
            property var player: Mpris.players.count > 0 ? Mpris.players.get(0) : null
            
            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 15
                visible: mediaBox.player !== null
                
                RowLayout {
                    spacing: 15
                    // Album Art Placeholder
                    Rectangle {
                        width: 60; height: 60; radius: 8
                        color: Config.accent; opacity: 0.5
                        Text { anchors.centerIn: parent; text: "󰎆"; color: "white"; font.pixelSize: 24 }
                    }
                    
                    Column {
                        Layout.fillWidth: true
                        Text { 
                            text: mediaBox.player ? mediaBox.player.metadata["xesam:title"] || "Unknown Title" : ""
                            color: Config.textPrimary; font.family: Config.mainFont; font.bold: true; elide: Text.ElideRight; width: 250 
                        }
                        Text { 
                            text: mediaBox.player ? mediaBox.player.metadata["xesam:artist"] || "Unknown Artist" : ""
                            color: Config.textSecondary; font.family: Config.mainFont; font.pixelSize: 12; elide: Text.ElideRight; width: 250 
                        }
                    }
                }
                
                RowLayout {
                    Layout.alignment: Qt.AlignHCenter
                    spacing: 30
                    Text { text: "󰒮"; color: "white"; font.pixelSize: 24; MouseArea { anchors.fill: parent; onClicked: mediaBox.player.previous() } }
                    Text { 
                        text: mediaBox.player && mediaBox.player.playbackStatus === Mpris.Playing ? "󰏤" : "󰐊"
                        color: "white"; font.pixelSize: 32
                        MouseArea { anchors.fill: parent; onClicked: mediaBox.player.playPause() }
                    }
                    Text { text: "󰒭"; color: "white"; font.pixelSize: 24; MouseArea { anchors.fill: parent; onClicked: mediaBox.player.next() } }
                }
            }
            
            ColumnLayout {
                anchors.centerIn: parent
                visible: mediaBox.player === null
                Text { text: "󰎆 No Media Playing"; color: Config.textSecondary; font.pixelSize: 16 }
            }
        }
    }
}
