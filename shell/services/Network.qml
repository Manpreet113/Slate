pragma Singleton
import QtQuick
import Quickshell.Io

Item {
    id: network
    
    property string ssid: "Disconnected"
    property bool connected: false
    
    function toggle() {
        let cmd = connected ? "off" : "on";
        toggleProc.run(["nmcli", "networking", cmd]);
    }
    
    Process {
        id: getProc
        command: ["nmcli", "-t", "-f", "active,ssid", "dev", "wifi"]
        running: true
        stdout: SplitParser {
            onRead: (data) => {
                let lines = data.split("\n");
                for (let line of lines) {
                    if (line.startsWith("yes:")) {
                        network.ssid = line.split(":")[1];
                        network.connected = true;
                        return;
                    }
                }
                network.ssid = "Disconnected";
                network.connected = false;
            }
        }
    }
    
    Process { id: toggleProc }
    
    Timer {
        interval: 10000
        running: true
        repeat: true
        onTriggered: getProc.run()
    }
}
