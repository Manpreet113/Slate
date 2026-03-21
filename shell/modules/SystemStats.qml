import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Services.UPower
import ".."

RowLayout {
    id: systemStats
    spacing: Config.padding * 1.5
    
    property string cpuUsage: "0%"
    property string ramUsage: "0G"
    
    readonly property string batteryText: UPower.displayDevice.percentage >= 0 
        ? Math.round(UPower.displayDevice.percentage) + "%" 
        : "--%"
    
    // CPU Fetcher
    Process {
        id: cpuProc
        command: ["sh", "-c", "top -bn1 | grep 'Cpu(s)' | awk '{print $2 + $4}'"]
        running: true
        stdout: SplitParser {
            onRead: (data) => {
                let val = parseFloat(data.trim());
                if (!isNaN(val)) cpuUsage = Math.round(val) + "%";
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
    
    Timer {
        interval: 2500
        running: true
        repeat: true
        onTriggered: {
            cpuProc.running = false; cpuProc.running = true;
            ramProc.running = false; ramProc.running = true;
        }
    }
    
    // Layout
    Text {
        text: "CPU " + cpuUsage
        color: Config.textSecondary
        font.family: Config.monoFont
        font.pixelSize: 11
    }
    
    Text {
        text: "RAM " + ramUsage
        color: Config.textSecondary
        font.family: Config.monoFont
        font.pixelSize: 11
    }
    
    Text {
        text: "BAT " + batteryText
        color: Config.accent
        font.family: Config.monoFont
        font.pixelSize: 12
        font.bold: true
    }
}
