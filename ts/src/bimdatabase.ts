export module BimDatabase {
    function submitAddEdit(
        form: HTMLFormElement,
        otherDataParent: HTMLElement,
        otherDataTextArea: HTMLTextAreaElement,
        ev: SubmitEvent,
    ) {
        ev.preventDefault();

        // reassemble text area value
        const otherEntryPieces: HTMLElement[] = Array.prototype.slice.call(otherDataParent.querySelectorAll("div.other-data-entry"), 0);
        const obj = {};
        for (let paragraph of otherEntryPieces) {
            const keyInput = <HTMLInputElement|null>paragraph.querySelector("input.key");
            if (keyInput === null) {
                continue;
            }

            const valueInput = <HTMLInputElement|null>paragraph.querySelector("input.value");
            if (valueInput === null) {
                continue;
            }

            obj[keyInput.value] = valueInput.value;
        }
        otherDataTextArea.value = JSON.stringify(obj);

        // remove custom form fields
        for (let paragraph of otherEntryPieces) {
            paragraph.parentNode?.removeChild(paragraph);
        }

        // submit modified form
        form.submit();
    }

    function addOtherDataEntry(otherDataParent: HTMLElement, newEntryContainer: HTMLElement): [HTMLInputElement, HTMLInputElement] {
        const entryContainer: HTMLElement = document.createElement("div");
        entryContainer.classList.add("other-data-entry");
        otherDataParent.insertBefore(entryContainer, newEntryContainer);

        const keyInput = document.createElement("input");
        keyInput.type = "text";
        keyInput.classList.add("key");
        entryContainer.appendChild(keyInput);

        const valueInput = document.createElement("input");
        valueInput.type = "text";
        valueInput.classList.add("value");
        entryContainer.appendChild(valueInput);

        const minusButton = document.createElement("input");
        minusButton.type = "button";
        minusButton.value = "\u2212";
        minusButton.addEventListener("click", () => entryContainer.parentNode?.removeChild(entryContainer));
        entryContainer.appendChild(minusButton);

        return [keyInput, valueInput];
    }

    function doSetUpAddEdit() {
        const otherDataTextArea = <HTMLTextAreaElement|null>document.getElementById("bimdb-ae-other-data");
        if (otherDataTextArea === null) {
            return;
        }
        const otherDataParent = otherDataTextArea.parentElement;
        if (otherDataParent === null) {
            return;
        }
        const form = otherDataTextArea.form;
        if (form === null) {
            return;
        }

        form.addEventListener("submit", ev => submitAddEdit(form, otherDataParent, otherDataTextArea, ev));

        // add new-entry piece
        const newEntryContainer = document.createElement("div");
        newEntryContainer.classList.add("add-other-data-entry");
        otherDataParent.appendChild(newEntryContainer);

        // disassemble text area
        const otherDataJson = JSON.parse(otherDataTextArea.value);
        const otherDataKeys = Object.keys(otherDataJson);
        for (let otherDataKey of otherDataKeys) {
            const otherDataValue = otherDataJson[otherDataKey];

            const [keyInput, valueInput] = addOtherDataEntry(otherDataParent, newEntryContainer);
            keyInput.value = otherDataKey;
            valueInput.value = otherDataValue;
        }

        const plusButton = document.createElement("input");
        plusButton.type = "button";
        plusButton.value = "+";
        plusButton.addEventListener("click", () => addOtherDataEntry(otherDataParent, newEntryContainer));
        newEntryContainer.appendChild(plusButton);

        otherDataTextArea.style.display = "none";
    }

    export function setUpAddEdit() {
        document.addEventListener("DOMContentLoaded", doSetUpAddEdit);
    }
}

// "globals are evil"
declare global {
    interface Window { BimDatabase: any; }
}
window.BimDatabase = BimDatabase;
