// Tiles foot left and "other" right, like sway does, for the guest test.
for (const w of workspace.stackingOrder) {
    if (!w.normalWindow) continue;
    const left = w.resourceClass === "foot";
    w.frameGeometry = { x: left ? 0 : 800, y: 0, width: 800, height: 900 };
}
