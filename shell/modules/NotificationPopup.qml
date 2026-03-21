import QtQuick
import QtQuick.Layouts
import QtQuick.Effects
import "../services" as Services
import ".."

Rectangle {
    id: popup
    
    property var notification: Services.Notifications.lastNotification
    
    visible: Services.Notifications.hasNotification
    width: 300
    height: 80
    radius: Config.radiusMedium
    color: Config.bg
    opacity: Config.bgOpacity + 0.2
    border.color: Config.borderColor
    
    layer.enabled: true
    layer.effect: MultiEffect {
        blurEnabled: true
        blur: Config.blurRadius
        shadowEnabled: true
        shadowOpacity: 0.5
        shadowBlur: 20
    }
    
    RowLayout {
        anchors.fill: parent
        anchors.margins: 15
        spacing: 15
        
        // Icon/App
        Rectangle {
            width: 40; height: 40; radius: 10
            color: Config.accent
            Text { anchors.centerIn: parent; text: "󰂚"; color: "white"; font.pixelSize: 20 }
        }
        
        Column {
            Layout.fillWidth: true
            Text { 
                text: popup.notification ? popup.notification.summary : "Notification"
                color: Config.textPrimary
                font.family: Config.mainFont
                font.bold: true
                elide: Text.ElideRight
                width: parent.width
            }
            Text { 
                text: popup.notification ? popup.notification.body : ""
                color: Config.textSecondary
                font.family: Config.mainFont
                font.pixelSize: 12
                elide: Text.ElideRight
                width: parent.width
            }
        }
    }
    
    Behavior on opacity { NumberAnimation { duration: 300 } }
}
