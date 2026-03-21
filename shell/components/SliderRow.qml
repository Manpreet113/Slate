import QtQuick
import QtQuick.Layouts
import ".."

RowLayout {
    id: root
    property string icon: ""
    property int value: 0
    signal moved(int val)
    
    spacing: 15
    Text { text: root.icon; color: Config.textPrimary; font.pixelSize: 20; Layout.preferredWidth: 30 }
    
    Rectangle {
        Layout.fillWidth: true
        height: 6; radius: 3
        color: "white"; opacity: 0.1
        
        Rectangle {
            width: parent.width * (root.value / 100)
            height: parent.height; radius: 3
            color: Config.accent
        }
        
        MouseArea {
            anchors.fill: parent
            onClicked: (mouse) => root.moved(Math.round((mouse.x / width) * 100))
            onPositionChanged: (mouse) => {
                if (pressed) root.moved(Math.round((mouse.x / width) * 100))
            }
        }
    }
}
