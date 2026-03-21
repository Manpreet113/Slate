import QtQuick
import Quickshell
import Quickshell.Io
import qs.modules.services
import qs.config
import qs.modules.globals

QtObject {
    id: root

    property Process compositorProcess: Process {}

    property var previousElysiumBinds: ({})
    property var previousCustomBinds: []
    property bool hasPreviousBinds: false

    property Timer applyTimer: Timer {
        interval: 100
        repeat: false
        onTriggered: applyKeybindsInternal()
    }

    function applyKeybinds() {
        applyTimer.restart();
    }

    // Helper function to check if an action is compatible with the current layout
    function isActionCompatibleWithLayout(action) {
        // If no layouts specified or empty array, action works in all layouts
        if (!action.layouts || action.layouts.length === 0)
            return true;

        // Check if current layout is in the allowed list
        const currentLayout = GlobalStates.compositorLayout;
        return action.layouts.indexOf(currentLayout) !== -1;
    }

    function cloneKeybind(keybind) {
        return {
            modifiers: keybind.modifiers ? keybind.modifiers.slice() : [],
            key: keybind.key || ""
        };
    }

    function storePreviousBinds() {
        if (!Config.keybindsLoader.loaded)
            return;

        const slate = Config.keybindsLoader.adapter.slate;

        // Store slate core keybinds
        previousElysiumBinds = {
            slate: {
                launcher: cloneKeybind(slate.launcher),
                dashboard: cloneKeybind(slate.dashboard),
                assistant: cloneKeybind(slate.assistant),
                clipboard: cloneKeybind(slate.clipboard),
                emoji: cloneKeybind(slate.emoji),
                notes: cloneKeybind(slate.notes),
                tmux: cloneKeybind(slate.tmux),
                wallpapers: cloneKeybind(slate.wallpapers)
            },
            system: {
                overview: cloneKeybind(slate.system.overview),
                powermenu: cloneKeybind(slate.system.powermenu),
                config: cloneKeybind(slate.system.config),
                lockscreen: cloneKeybind(slate.system.lockscreen),
                tools: cloneKeybind(slate.system.tools),
                screenshot: cloneKeybind(slate.system.screenshot),
                screenrecord: cloneKeybind(slate.system.screenrecord),
                lens: cloneKeybind(slate.system.lens),
                reload: slate.system.reload ? cloneKeybind(slate.system.reload) : null,
                quit: slate.system.quit ? cloneKeybind(slate.system.quit) : null
            }
        };

        // Store custom keybinds
        const customBinds = Config.keybindsLoader.adapter.custom;
        previousCustomBinds = [];
        if (customBinds && customBinds.length > 0) {
            for (let i = 0; i < customBinds.length; i++) {
                const bind = customBinds[i];
                if (bind.keys) {
                    let keys = [];
                    for (let k = 0; k < bind.keys.length; k++) {
                        keys.push(cloneKeybind(bind.keys[k]));
                    }
                    previousCustomBinds.push({
                        keys: keys
                    });
                } else {
                    previousCustomBinds.push(cloneKeybind(bind));
                }
            }
        }

        hasPreviousBinds = true;
    }

    // Build an unbind target object (modifiers + key only).
    function makeUnbindTarget(keybind) {
        return {
            modifiers: keybind.modifiers || [],
            key: keybind.key || ""
        };
    }

    // Build a structured bind object from a core keybind (has all fields inline).
    function makeBindFromCore(keybind) {
        return {
            modifiers: keybind.modifiers || [],
            key: keybind.key || "",
            dispatcher: keybind.dispatcher || "",
            argument: keybind.argument || "",
            flags: keybind.flags || "",
            enabled: true
        };
    }

    // Build a structured bind object from a key + action pair (custom keybinds).
    function makeBindFromKeyAction(keyObj, action) {
        return {
            modifiers: keyObj.modifiers || [],
            key: keyObj.key || "",
            dispatcher: action.dispatcher || "",
            argument: action.argument || "",
            flags: action.flags || "",
            enabled: true
        };
    }

    function applyKeybindsInternal() {
        // Ensure adapter is loaded.
        if (!Config.keybindsLoader.loaded) {
            console.log("CompositorKeybinds: Esperando que se cargue el adapter...");
            return;
        }

        // Wait for layout to be ready.
        if (!GlobalStates.compositorLayoutReady) {
            console.log("CompositorKeybinds: Esperando que se detecte el layout de AxctlService...");
            return;
        }

        console.log("CompositorKeybinds: Aplicando keybindings (layout: " + GlobalStates.compositorLayout + ")...");

        // Build structured payload.
        let payload = { binds: [], unbinds: [] };

        // First, unbind previous keybinds if we have them stored
        if (hasPreviousBinds) {
            // Unbind previous slate core keybinds
            if (previousElysiumBinds.slate) {
                payload.unbinds.push(makeUnbindTarget(previousElysiumBinds.slate.launcher));
                payload.unbinds.push(makeUnbindTarget(previousElysiumBinds.slate.dashboard));
                payload.unbinds.push(makeUnbindTarget(previousElysiumBinds.slate.assistant));
                payload.unbinds.push(makeUnbindTarget(previousElysiumBinds.slate.clipboard));
                payload.unbinds.push(makeUnbindTarget(previousElysiumBinds.slate.emoji));
                payload.unbinds.push(makeUnbindTarget(previousElysiumBinds.slate.notes));
                payload.unbinds.push(makeUnbindTarget(previousElysiumBinds.slate.tmux));
                payload.unbinds.push(makeUnbindTarget(previousElysiumBinds.slate.wallpapers));
            }

            // Unbind previous slate system keybinds
            if (previousElysiumBinds.system) {
                payload.unbinds.push(makeUnbindTarget(previousElysiumBinds.system.overview));
                payload.unbinds.push(makeUnbindTarget(previousElysiumBinds.system.powermenu));
                payload.unbinds.push(makeUnbindTarget(previousElysiumBinds.system.config));
                payload.unbinds.push(makeUnbindTarget(previousElysiumBinds.system.lockscreen));
                payload.unbinds.push(makeUnbindTarget(previousElysiumBinds.system.tools));
                payload.unbinds.push(makeUnbindTarget(previousElysiumBinds.system.screenshot));
                payload.unbinds.push(makeUnbindTarget(previousElysiumBinds.system.screenrecord));
                payload.unbinds.push(makeUnbindTarget(previousElysiumBinds.system.lens));
                if (previousElysiumBinds.system.reload) payload.unbinds.push(makeUnbindTarget(previousElysiumBinds.system.reload));
                if (previousElysiumBinds.system.quit) payload.unbinds.push(makeUnbindTarget(previousElysiumBinds.system.quit));
            }

            // Unbind previous custom keybinds
            for (let i = 0; i < previousCustomBinds.length; i++) {
                const prev = previousCustomBinds[i];
                if (prev.keys) {
                    for (let k = 0; k < prev.keys.length; k++) {
                        payload.unbinds.push(makeUnbindTarget(prev.keys[k]));
                    }
                } else {
                    payload.unbinds.push(makeUnbindTarget(prev));
                }
            }
        }

        // Process core keybinds.
        const slate = Config.keybindsLoader.adapter.slate;

        // Unbind current core keybinds (ensures clean state before rebinding)
        payload.unbinds.push(makeUnbindTarget(slate.launcher));
        payload.unbinds.push(makeUnbindTarget(slate.dashboard));
        payload.unbinds.push(makeUnbindTarget(slate.assistant));
        payload.unbinds.push(makeUnbindTarget(slate.clipboard));
        payload.unbinds.push(makeUnbindTarget(slate.emoji));
        payload.unbinds.push(makeUnbindTarget(slate.notes));
        payload.unbinds.push(makeUnbindTarget(slate.tmux));
        payload.unbinds.push(makeUnbindTarget(slate.wallpapers));

        // Bind current core keybinds
        payload.binds.push(makeBindFromCore(slate.launcher));
        payload.binds.push(makeBindFromCore(slate.dashboard));
        payload.binds.push(makeBindFromCore(slate.assistant));
        payload.binds.push(makeBindFromCore(slate.clipboard));
        payload.binds.push(makeBindFromCore(slate.emoji));
        payload.binds.push(makeBindFromCore(slate.notes));
        payload.binds.push(makeBindFromCore(slate.tmux));
        payload.binds.push(makeBindFromCore(slate.wallpapers));

        // System keybinds
        const system = slate.system;

        // Unbind current system keybinds
        payload.unbinds.push(makeUnbindTarget(system.overview));
        payload.unbinds.push(makeUnbindTarget(system.powermenu));
        payload.unbinds.push(makeUnbindTarget(system.config));
        payload.unbinds.push(makeUnbindTarget(system.lockscreen));
        payload.unbinds.push(makeUnbindTarget(system.tools));
        payload.unbinds.push(makeUnbindTarget(system.screenshot));
        payload.unbinds.push(makeUnbindTarget(system.screenrecord));
        payload.unbinds.push(makeUnbindTarget(system.lens));
        if (system.reload) payload.unbinds.push(makeUnbindTarget(system.reload));
        if (system.quit) payload.unbinds.push(makeUnbindTarget(system.quit));

        // Bind current system keybinds
        payload.binds.push(makeBindFromCore(system.overview));
        payload.binds.push(makeBindFromCore(system.powermenu));
        payload.binds.push(makeBindFromCore(system.config));
        payload.binds.push(makeBindFromCore(system.lockscreen));
        payload.binds.push(makeBindFromCore(system.tools));
        payload.binds.push(makeBindFromCore(system.screenshot));
        payload.binds.push(makeBindFromCore(system.screenrecord));
        payload.binds.push(makeBindFromCore(system.lens));
        if (system.reload) payload.binds.push(makeBindFromCore(system.reload));
        if (system.quit) payload.binds.push(makeBindFromCore(system.quit));

        // Process custom keybinds (keys[] and actions[] format).
        const customBinds = Config.keybindsLoader.adapter.custom;
        if (customBinds && customBinds.length > 0) {
            for (let i = 0; i < customBinds.length; i++) {
                const bind = customBinds[i];

                // Check if bind has the new format
                if (bind.keys && bind.actions) {
                    // Unbind all keys first (always unbind regardless of layout)
                    for (let k = 0; k < bind.keys.length; k++) {
                        payload.unbinds.push(makeUnbindTarget(bind.keys[k]));
                    }

                    // Only create binds if enabled
                    if (bind.enabled !== false) {
                        // For each key, bind only compatible actions
                        for (let k = 0; k < bind.keys.length; k++) {
                            for (let a = 0; a < bind.actions.length; a++) {
                                const action = bind.actions[a];
                                // Check if this action is compatible with the current layout
                                if (isActionCompatibleWithLayout(action)) {
                                    payload.binds.push(makeBindFromKeyAction(bind.keys[k], action));
                                }
                            }
                        }
                    }
                } else {
                    // Fallback for old format (shouldn't happen after normalization)
                    payload.unbinds.push(makeUnbindTarget(bind));
                    if (bind.enabled !== false) {
                        payload.binds.push(makeBindFromCore(bind));
                    }
                }
            }
        }

        storePreviousBinds();

        // Send structured payload via axctl keybinds-batch.
        console.log("CompositorKeybinds: Enviando keybinds-batch (" + payload.unbinds.length + " unbinds, " + payload.binds.length + " binds)");
        compositorProcess.command = ["axctl", "config", "keybinds-batch", JSON.stringify(payload)];
        compositorProcess.running = true;
    }

    property Connections configConnections: Connections {
        target: Config.keybindsLoader
        function onFileChanged() {
            applyKeybinds();
        }
        function onLoaded() {
            applyKeybinds();
        }
        function onAdapterUpdated() {
            applyKeybinds();
        }
    }

    // Re-apply keybinds when layout changes
    property Connections globalStatesConnections: Connections {
        target: GlobalStates
        function onCompositorLayoutChanged() {
            console.log("CompositorKeybinds: Layout changed to " + GlobalStates.compositorLayout + ", reapplying keybindings...");
            applyKeybinds();
        }
        function onCompositorLayoutReadyChanged() {
            if (GlobalStates.compositorLayoutReady) {
                applyKeybinds();
            }
        }
    }

    property Connections compositorConnections: Connections {
        target: AxctlService
        function onRawEvent(event) {
            if (event.name === "configreloaded") {
                console.log("CompositorKeybinds: Detectado configreloaded, reaplicando keybindings...");
                applyKeybinds();
            }
        }
    }

    Component.onCompleted: {
        // Apply immediately if loader is ready.
        if (Config.keybindsLoader.loaded) {
            applyKeybinds();
        }
    }
}
