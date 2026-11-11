// Answers wisp's questions over D-Bus: which window is under the cursor,
// and focus this one. GNOME offers no other way to ask.

import Gio from 'gi://Gio';
import Meta from 'gi://Meta';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

const NAME = 'io.github.tkutsu.Wisp.Shell';
const PATH = '/io/github/tkutsu/Wisp/Shell';
const INTERFACE = `
<node>
  <interface name="${NAME}">
    <method name="PointerTarget">
      <arg type="s" direction="out" name="json"/>
    </method>
    <method name="Focus">
      <arg type="s" direction="in" name="id"/>
      <arg type="b" direction="out" name="done"/>
    </method>
  </interface>
</node>`;

const TYPES = [Meta.WindowType.NORMAL, Meta.WindowType.DIALOG, Meta.WindowType.MODAL_DIALOG];

function windows() {
    return global.get_window_actors().map(actor => actor.meta_window);
}

export default class WispExtension extends Extension {
    enable() {
        this._object = Gio.DBusExportedObject.wrapJSObject(INTERFACE, this);
        this._object.export(Gio.DBus.session, PATH);
        this._name = Gio.bus_own_name_on_connection(Gio.DBus.session, NAME, Gio.BusNameOwnerFlags.NONE, null, null);
    }

    disable() {
        Gio.bus_unown_name(this._name);
        this._object.unexport();
        this._object = null;
    }

    PointerTarget() {
        const [x, y] = global.get_pointer();
        const workspace = global.workspace_manager.get_active_workspace();
        // The overview covers every window. Window actors come in stacking
        // order, bottom first.
        const window = Main.overview.visible ? null : windows().reverse().find(w => {
            const r = w.get_frame_rect();
            return TYPES.includes(w.get_window_type()) &&
                !w.minimized &&
                w.located_on_workspace(workspace) &&
                x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height;
        });
        return JSON.stringify({
            id: window ? String(window.get_id()) : '',
            class: window?.get_wm_class() ?? '',
            title: window?.get_title() ?? '',
        });
    }

    Focus(id) {
        const window = windows().find(w => String(w.get_id()) === id);
        window?.activate(global.get_current_time());
        return Boolean(window);
    }
}
