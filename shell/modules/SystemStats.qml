import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Services.UPower
import ".."

RowLayout {
    id: systemStats
    spacing: Config.padding
    
    property string cpuUsage: "0%"
    property string ramUsage: "0G"
    
    UPower { id: upower }
    
    readonly property string batteryText: upower.displayDevice.percentage >= 0 
        ? Math.round(upower.displayDevice.percentage) + "%" 
        : "N/A"
    
    // CPU Fetcher: Using sh -c to run a pipeline
    Process {
        id: cpuProc
        command: ["sh", "-c", "top -bn1 | grep 'Cpu(s)' | awk '{print $2 + $4}'"]
        running: true
        stdout: SplitParser {
            onRead: (data) => {
                let val = parseFloat(data.trim());
                if (!isNaN(val)) {
                    cpuUsage = Math.round(val) + "%";
                }
            }
        }
    }
    
    // RAM Fetcher
    Process {
        id: ramProc
        command: ["sh", "-c", "free -h | grep Mem | awk '{print $3}'"]
        running: true
        stdout: SplitParser {
            onRead: (data) => {
                ramUsage = data.trim();
            }
        }
    }
    
    // Update every 2 seconds
    Timer {
        interval: 2000
        running: true
        repeat: true
        onTriggered: {
            cpuProc.running = false;
            ramProc.running = false;
            cpuProc.running = true;
            ramProc.running = true;
        }
    }
    
    Text {
        text: "CPU: " + cpuUsage
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
        text: "BAT: " + batteryText
        color: Config.fg
        opacity: 0.8
        font.family: Config.sansFont
        font.pointSize: Config.fontSize - 1
    }
}
