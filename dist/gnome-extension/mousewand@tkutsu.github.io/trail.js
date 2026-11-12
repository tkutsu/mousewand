// The gesture trail, drawn inside GNOME Shell: GNOME has no layer-shell,
// so this is the only place that can draw above every window.
//
// It follows mousewand's own trail: a ribbon that narrows and fades with each
// point's age, a soft glow around a solid body, rings on repeat steps,
// and a fade on release, red when nothing matched. The canvas covers only
// the stroke, since Cairo redraws it on the CPU every frame.

import Cairo from 'gi://cairo';
import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

import {Hint} from './hint.js';

const DEFAULT_THEME = {
    color: [0.45, 0.75, 1.0],
    rejected: [1.0, 0.35, 0.35],
    width: 26,
    lifeMs: 450,
    fadeMs: 300,
    glow: 0.6,
};
const RING_LIFE_MS = 300;
const RING_REACH = 40;
const MIN_STEP = 1.5;

const now = () => GLib.get_monotonic_time() / 1000;

export class Trail {
    constructor() {
        this._theme = DEFAULT_THEME;
        this._area = null;
        this.hint = new Hint();
    }

    showHint(hint) {
        if (this._area)
            this.hint.show(hint);
    }

    setTheme(theme) {
        this._theme = {...DEFAULT_THEME, ...theme};
    }

    begin() {
        this.destroy();
        this._points = [];
        this._pulses = [];
        this._ended = null;
        this._matched = true;
        this._origin = [0, 0];
        this._area = new St.DrawingArea({reactive: false});
        Main.layoutManager.uiGroup.add_child(this._area);
        this._area.connect('repaint', area => this._paint(area));
        // A timeline ticks with the compositor's frame clock.
        this._timeline = new Clutter.Timeline({actor: this._area, duration: 1000, repeatCount: -1});
        this._timeline.connect('new-frame', () => this._frame());
        this._timeline.start();
    }

    tick() {
        const head = this._points?.at(-1);
        if (this._area && head)
            this._pulses.push({x: head.x, y: head.y, t: now()});
    }

    end(matched) {
        if (!this._area || this._ended)
            return;
        this._ended = now();
        this._matched = matched;
    }

    destroy() {
        this.hint.destroy();
        this._timeline?.stop();
        this._timeline = null;
        this._area?.destroy();
        this._area = null;
    }

    _frame() {
        const t = now();
        if (!this._ended) {
            const [x, y] = global.get_pointer();
            const last = this._points.at(-1);
            if (!last || Math.hypot(last.x - x, last.y - y) >= MIN_STEP)
                this._points.push({x, y, t});
        }
        this._points = this._points.filter(p => t - p.t < this._theme.lifeMs);
        this._pulses = this._pulses.filter(p => t - p.t < RING_LIFE_MS);
        this._fade = this._ended ? 1 - (t - this._ended) / Math.max(this._theme.fadeMs, 1) : 1;
        if (this._fade <= 0) {
            this.destroy();
            return;
        }
        this.hint.fade(this._fade);

        const spots = [...this._points, ...this._pulses];
        if (spots.length === 0) {
            this._area.hide();
            return;
        }
        const pad = this._theme.width + RING_REACH;
        const xs = spots.map(p => p.x);
        const ys = spots.map(p => p.y);
        const [x0, y0] = [Math.min(...xs) - pad, Math.min(...ys) - pad];
        this._origin = [x0, y0];
        this._area.set_position(x0, y0);
        this._area.set_size(Math.max(...xs) + pad - x0, Math.max(...ys) + pad - y0);
        this._area.show();
        this._area.queue_repaint();
    }

    _paint(area) {
        const cr = area.get_context();
        const t = now();
        const theme = this._theme;
        const [r, g, b] = this._ended && !this._matched ? theme.rejected : theme.color;
        const [ox, oy] = this._origin;
        const life = p => Math.max(0, 1 - (t - p.t) / theme.lifeMs);

        cr.setOperator(Cairo.Operator.CLEAR);
        cr.paint();
        cr.setLineCap(Cairo.LineCap.ROUND);
        cr.setLineJoin(Cairo.LineJoin.ROUND);

        // Each pass goes into a group with SOURCE, so overlapping segments
        // replace instead of stacking alpha at every joint.
        const pass = (widthScale, rgb, alphaScale) => {
            cr.pushGroup();
            cr.setOperator(Cairo.Operator.SOURCE);
            for (let i = 1; i < this._points.length; i++) {
                const [a, p] = [this._points[i - 1], this._points[i]];
                const l = life(p);
                cr.setLineWidth(Math.max(theme.width * Math.pow(l, 0.7) * widthScale, 0.5));
                cr.setSourceRGBA(...rgb, l * this._fade * alphaScale);
                cr.moveTo(a.x - ox, a.y - oy);
                cr.lineTo(p.x - ox, p.y - oy);
                cr.stroke();
            }
            cr.popGroupToSource();
            cr.setOperator(Cairo.Operator.OVER);
            cr.paint();
        };
        const light = c => c + (1 - c) * 0.3;
        // Nested strokes, wide and faint to narrow and strong, stand in for
        // the shader's glow falloff, then the body and its lighter middle.
        for (const [scale, alpha] of [[1.0, 0.08], [0.7, 0.14], [0.45, 0.25]])
            pass(scale, [r, g, b], theme.glow * alpha);
        pass(0.32, [r, g, b], 1);
        pass(0.12, [light(r), light(g), light(b)], 1);

        for (const pulse of this._pulses) {
            const progress = Math.min((t - pulse.t) / RING_LIFE_MS, 1);
            cr.setLineWidth(2 * (7 * (1 - progress) + 2) * 0.5);
            cr.setSourceRGBA(r, g, b, (1 - progress) * this._fade);
            cr.arc(pulse.x - ox, pulse.y - oy, 8 + 28 * Math.sqrt(progress), 0, 2 * Math.PI);
            cr.stroke();
        }
        cr.$dispose();
    }
}
