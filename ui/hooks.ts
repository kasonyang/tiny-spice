import {useState} from "react";

export interface InputProps {
    text: string,
    onTextChange: (e: ITextEvent) => void,
}

export function useInput(initial = ""): [InputProps, (string) => void] {
    const [value, setValue] = useState(initial);

    function onTextChange(e: ITextEvent) {
        setValue(e.detail.value);
    }

    const props = {
        text: value,
        onTextChange,
    }
    return [props, setValue]
}