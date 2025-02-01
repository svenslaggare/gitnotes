"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.OptionalRender = OptionalRender;
function OptionalRender(_a) {
    var condition = _a.condition, trueContent = _a.trueContent, falseContent = _a.falseContent;
    if (condition) {
        return trueContent;
    }
    else {
        return falseContent;
    }
}
