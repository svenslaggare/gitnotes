import {JSX} from "react";

export function OptionalRender({ condition, trueContent, falseContent }: { condition: boolean; trueContent: JSX.Element; falseContent: JSX.Element }) {
    if (condition) {
        return trueContent;
    } else {
        return falseContent;
    }
}