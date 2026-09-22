const { Gio, GLib, Shell } = imports.gi;
const Main = imports.ui.main;

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

const SnapshotIface = `<node>
  <interface name="org.patina.WindowTracker1">
    <method name="GetSnapshot">
      <arg name="version" type="u" direction="out"/>
      <arg name="state" type="u" direction="out"/>
      <arg name="title" type="s" direction="out"/>
      <arg name="desktop_id" type="s" direction="out"/>
      <arg name="wm_class" type="s" direction="out"/>
      <arg name="pid" type="u" direction="out"/>
      <arg name="window_id" type="s" direction="out"/>
    </method>
  </interface>
</node>`;

let tracker = null;
function init() {}
function enable() {
    if (!tracker)
        tracker = new PatinaTracker();
    tracker.start();
}
function disable() {
    if (tracker)
        tracker.stop();
    tracker = null;
}

function emptySnapshot(state) {
    return [1, state, '', '', '', 0, ''];
}

// Bound UTF-8 bytes without splitting a code point or exporting invalid D-Bus text.
function boundedText(value, limit) {
    if (value === null || value === undefined)
        return '';
    if (typeof value !== 'string')
        throw new Error('invalid-text');
    let result = '';
    let bytes = 0;
    for (const char of value) {
        const point = char.codePointAt(0);
        if (point === 0 || (point >= 0xd800 && point <= 0xdfff))
            continue;
        const size = point <= 0x7f ? 1 : point <= 0x7ff ? 2 : point <= 0xffff ? 3 : 4;
        if (bytes + size > limit)
            break;
        result += char;
        bytes += size;
    }
    return result;
}

function legacyTuple(snapshot) {
    if (snapshot[1] !== 1)
        return ['', '', '', 0, 0];
    return [snapshot[2], snapshot[3].replace(/\.desktop$/, ''),
        snapshot[4], snapshot[5], Number(snapshot[6])];
}

class PatinaTracker {
    constructor() {
        this._generation = 0;
        this._enabled = false;
        this._endpoints = [];
        this._connections = [];
        this._timeoutId = null;
    }

    start() {
        if (this._enabled)
            return;
        this._enabled = true;
        const generation = ++this._generation;
        const current = () => this._enabled && this._generation === generation;
        try {
            for (const [source, signal] of [
                [global.display, 'notify::focus-window'],
                [Main.sessionMode, 'updated'],
                [Main.screenShield, 'locked-changed'],
                [Main.screenShield, 'active-changed'],
                [Main.overview, 'showing'],
                [Main.overview, 'hidden'],
            ]) {
                if (!source)
                    continue;
                const id = source.connect(signal, () => {
                    if (current())
                        this._onFocusChanged();
                });
                this._connections.push([source, id]);
            }
            for (const [name, path, xml, methods] of [
                [BUS_NAME, OBJECT_PATH, PatinaIface,
                    { GetFocusedWindow: () => legacyTuple(this._snapshot()) }],
                ['org.patina.WindowTracker1', '/org/patina/WindowTracker1', SnapshotIface,
                    { GetSnapshot: () => this._snapshot() }],
            ]) {
                const endpoint = { owner: null, object: null, last: null, name };
                this._endpoints.push(endpoint);
                endpoint.owner = Gio.bus_own_name(Gio.BusType.SESSION, name,
                    Gio.BusNameOwnerFlags.NONE, null,
                    connection => {
                        if (!current())
                            return;
                        this._unexport(endpoint);
                        try {
                            endpoint.object = Gio.DBusExportedObject.wrapJSObject(xml, methods);
                            endpoint.object.export(connection, path);
                            this._onFocusChanged();
                        } catch (_) {
                            this._unexport(endpoint);
                            log('PATINA: D-Bus export failed');
                        }
                    },
                    () => {
                        if (current())
                            this._unexport(endpoint);
                    });
            }
            this._timeoutId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 500, () => {
                if (current()) {
                    this._timeoutId = null;
                    this._onFocusChanged();
                }
                return GLib.SOURCE_REMOVE;
            });
        } catch (_) {
            this.stop();
            log('PATINA: extension startup failed');
        }
    }

    _unexport(endpoint) {
        if (endpoint.object)
            endpoint.object.unexport();
        endpoint.object = null;
        endpoint.last = null;
    }

    stop() {
        this._enabled = false;
        ++this._generation;
        if (this._timeoutId !== null)
            GLib.source_remove(this._timeoutId);
        this._timeoutId = null;
        for (const [source, id] of this._connections)
            source.disconnect(id);
        this._connections = [];
        for (const endpoint of this._endpoints) {
            this._unexport(endpoint);
            if (endpoint.owner !== null)
                Gio.bus_unown_name(endpoint.owner);
        }
        this._endpoints = [];
    }

    _snapshot() {
        if (!this._enabled)
            return emptySnapshot(3);
        try {
            // Do not even read focus_window behind the shield or overview.
            if (Main.sessionMode.isLocked || Main.screenShield?.locked || Main.screenShield?.active)
                return emptySnapshot(2);
            if (Main.overview.visible || Main.overview.visibleTarget)
                return emptySnapshot(0);
            const win = global.display.focus_window;
            if (!win)
                return emptySnapshot(0);
            const pid = win.get_pid();
            const id = win.get_id();
            if (!Number.isInteger(pid) || pid < 0 || pid > 0xffffffff ||
                !Number.isSafeInteger(id) || id < 0)
                return emptySnapshot(3);
            const app = Shell.WindowTracker.get_default().get_window_app(win);
            const desktopId = boundedText(app ? app.get_id() : '', 512);
            const wmClass = boundedText(win.get_wm_class(), 512);
            if (!pid && !desktopId.trim())
                return emptySnapshot(3);
            return [1, 1, boundedText(win.get_title(), 4096), desktopId,
                wmClass, pid, String(id)];
        } catch (_) {
            // Exceptions may contain activity data; never log their text.
            return emptySnapshot(3);
        }
    }

    _onFocusChanged() {
        const endpoint = this._endpoints.find(item => item.name === BUS_NAME);
        if (!endpoint || !endpoint.object)
            return;
        const values = legacyTuple(this._snapshot());
        const key = JSON.stringify(values);
        if (endpoint.last === key)
            return;
        endpoint.object.emit_signal('FocusedWindowChanged', new GLib.Variant('(sssut)', values));
        endpoint.last = key;
    }
}
