const { Gio, GLib, Shell } = imports.gi;

const BUS_NAME = 'org.patina.WindowTracker';
const OBJECT_PATH = '/org/patina/WindowTracker';

const PatinaIface = `
<node>
  <interface name="org.patina.WindowTracker">
    <method name="GetFocusedWindow">
      <arg name="title" type="s" direction="out"/>
      <arg name="app_id" type="s" direction="out"/>
      <arg name="wm_class" type="s" direction="out"/>
      <arg name="pid" type="u" direction="out"/>
      <arg name="window_id" type="t" direction="out"/>
    </method>
    <signal name="FocusedWindowChanged">
      <arg name="title" type="s"/>
      <arg name="app_id" type="s"/>
      <arg name="wm_class" type="s"/>
      <arg name="pid" type="u"/>
      <arg name="window_id" type="t"/>
    </signal>
  </interface>
</node>`;

let tracker = null;

function init() {}

function enable() {
    if (!tracker) {
        tracker = new PatinaTracker();
    }
    tracker.start();
}

function disable() {
    if (tracker) {
        tracker.stop();
        tracker = null;
    }
}

class PatinaTracker {
    constructor() {
        this._nameOwnerId = null;
        this._exportedObject = null;
        this._focusHandlerId = null;
        this._lastFocused = null;
        this._timeoutId = null;
        this._nodeInfo = Gio.DBusNodeInfo.new_for_xml(PatinaIface);
    }

    start() {
        this._nameOwnerId = Gio.bus_own_name(
            Gio.BusType.SESSION,
            BUS_NAME,
            Gio.BusNameOwnerFlags.NONE,
            this._onBusAcquired.bind(this),
            this._onNameAcquired.bind(this),
            this._onNameLost.bind(this)
        );
        log('PATINA: bus_own_name returned ' + this._nameOwnerId);
    }

    stop() {
        if (this._timeoutId) {
            GLib.source_remove(this._timeoutId);
            this._timeoutId = null;
        }
        if (this._focusHandlerId) {
            global.display.disconnect(this._focusHandlerId);
            this._focusHandlerId = null;
        }
        if (this._exportedObject) {
            this._exportedObject.unexport();
            this._exportedObject = null;
        }
        if (this._nameOwnerId) {
            Gio.bus_unown_name(this._nameOwnerId);
            this._nameOwnerId = null;
        }
        this._lastFocused = null;
    }

    _onBusAcquired(connection, name) {
        log('PATINA: bus acquired: ' + name);

        const self = this;
        const impl = {
            GetFocusedWindow() {
                log('PATINA: GetFocusedWindow called');
                const info = self._getWindowInfo();
                log('PATINA: info title=' + info.title + ' app=' + info.app_id);
                return [info.title, info.app_id, info.wm_class, info.pid, info.window_id];
            }
        };

        this._exportedObject = Gio.DBusExportedObject.wrapJSObject(
            this._nodeInfo.interfaces[0],
            impl
        );
        this._exportedObject.export(connection, OBJECT_PATH);
        log('PATINA: object exported to ' + OBJECT_PATH);

        this._focusHandlerId = global.display.connect(
            'notify::focus-window',
            () => this._onFocusChanged()
        );

        this._timeoutId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 500, () => {
            this._onFocusChanged();
            this._timeoutId = null;
            return GLib.SOURCE_REMOVE;
        });
    }

    _onNameAcquired(connection, name) {
        log('PATINA: name acquired: ' + name);
    }

    _onNameLost(connection, name) {
        log('PATINA: name lost: ' + name);
    }

    _getWindowInfo() {
        const win = global.display.focus_window;
        if (!win) {
            return { title: '', app_id: '', wm_class: '', pid: 0, window_id: 0 };
        }

        let app_id = '';
        const app = Shell.WindowTracker.get_default().get_window_app(win);
        if (app) {
            app_id = app.get_id().replace('.desktop', '');
        }

        return {
            title: win.get_title() || '',
            app_id: app_id,
            wm_class: win.get_wm_class() || '',
            pid: win.get_pid(),
            window_id: win.get_id()
        };
    }

    _onFocusChanged() {
        const info = this._getWindowInfo();
        const key = `${info.window_id}:${info.title}`;
        if (this._lastFocused === key) return;
        this._lastFocused = key;

        if (this._exportedObject) {
            this._exportedObject.emit_signal(
                'FocusedWindowChanged',
                new GLib.Variant(
                    '(sstut)',
                    [info.title, info.app_id, info.wm_class, info.pid, info.window_id]
                )
            );
        }
    }
}
