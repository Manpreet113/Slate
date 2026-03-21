import QtQuick
import QtQuick.Layouts
import Quickshell
import ".."

RowLayout {
    id: systemStats
    spacing: Config.padding
    
    // Placeholder for real logic (CPU/RAM usually require a C++ provider or script)
    // QuickShell allows running scripts to get data.
    
    Text {
        text: "CPU: 14%"
        color: Config.fg
        opacity: 0.8
        font.family: Config.sansFont
        font.pointSize: Config.fontSize - 1
    }
    
    Rectangle {
        width: 1
        height: 12
        color: Config.fg
        opacity: 0.2
    }
    
    Text {
        text: "RAM: 3.8G"
        color: Config.fg
        opacity: 0.8
        font.family: Config.sansFont
        font.pointSize: Config.fontSize - 1
    }
}
