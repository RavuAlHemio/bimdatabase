export module CouplingAddEdit {
    function handleSubmit(
        form: HTMLFormElement,
        vehiclesParent: HTMLElement,
        vehiclesTextArea: HTMLTextAreaElement,
        ev: SubmitEvent,
    ) {
        ev.preventDefault();

        // reassemble text area value
        const vehicleEntries: HTMLElement[] = Array.prototype.slice.call(vehiclesParent.querySelectorAll(".vehicle-entry"), 0);
        const numbers: string[] = [];
        for (let vehicleEntry of vehicleEntries) {
            const numberInput = <HTMLInputElement|null>vehicleEntry.querySelector("input.vehicle-number");
            if (numberInput === null) {
                continue;
            }
            numbers.push(numberInput.value);
        }
        vehiclesTextArea.value = numbers.join("\n");

        // remove custom form fields
        for (let vehicleEntry of vehicleEntries) {
            vehicleEntry.parentNode?.removeChild(vehicleEntry);
        }

        // submit modified form
        form.submit();
    }

    function enableDisableUpDown(vehiclesParent: HTMLElement) {
        const vehicleEntries = vehiclesParent.querySelectorAll(".vehicle-entry");
        for (let i = 0; i < vehicleEntries.length; i++) {
            const vehicleEntry = vehicleEntries.item(i);
            const upButton = <HTMLInputElement|null>vehicleEntry.querySelector(".up-button");
            if (upButton !== null) {
                upButton.disabled = (i === 0);
            }
            const downButton = <HTMLInputElement|null>vehicleEntry.querySelector(".down-button");
            if (downButton !== null) {
                downButton.disabled = (i === vehicleEntries.length - 1);
            }
        }
    }

    function moveUp(vehiclesParent: HTMLElement, entryContainer: HTMLElement) {
        entryContainer.parentNode?.insertBefore(entryContainer, entryContainer.previousElementSibling);
        enableDisableUpDown(vehiclesParent);
    }

    function moveDown(vehiclesParent: HTMLElement, entryContainer: HTMLElement) {
        const next = entryContainer.nextElementSibling;
        const nextNext = (next !== null) ? next.nextElementSibling : null;
        entryContainer.parentNode?.insertBefore(entryContainer, nextNext);
        enableDisableUpDown(vehiclesParent);
    }

    function remove(vehiclesParent: HTMLElement, entryContainer: HTMLElement) {
        entryContainer.parentNode?.removeChild(entryContainer);
        enableDisableUpDown(vehiclesParent);
    }

    function addVehicle(vehiclesParent: HTMLElement, newEntryContainer: HTMLElement): HTMLInputElement {
        const entryContainer: HTMLElement = document.createElement("div");
        entryContainer.classList.add("vehicle-entry");
        vehiclesParent.insertBefore(entryContainer, newEntryContainer);

        const numberInput = document.createElement("input");
        numberInput.type = "text";
        numberInput.classList.add("vehicle-number");
        entryContainer.appendChild(numberInput);

        const minusButton = document.createElement("input");
        minusButton.type = "button";
        minusButton.value = "\u2212";
        minusButton.addEventListener("click", () => remove(vehiclesParent, entryContainer));
        entryContainer.appendChild(minusButton);

        const upButton = document.createElement("input");
        upButton.type = "button";
        upButton.classList.add("up-button");
        upButton.value = "\u2191";
        upButton.addEventListener("click", () => moveUp(vehiclesParent, entryContainer));
        entryContainer.appendChild(upButton);

        const downButton = document.createElement("input");
        downButton.type = "button";
        downButton.classList.add("down-button");
        downButton.value = "\u2193";
        downButton.addEventListener("click", () => moveDown(vehiclesParent, entryContainer));
        entryContainer.appendChild(downButton);

        enableDisableUpDown(vehiclesParent);

        return numberInput;
    }

    export function doSetUp() {
        const vehiclesTextArea = <HTMLTextAreaElement|null>document.getElementById("bimdb-cae-vehicles");
        if (vehiclesTextArea === null) {
            return;
        }
        const vehiclesParent = vehiclesTextArea.parentElement;
        if (vehiclesParent === null) {
            return;
        }
        const form = vehiclesTextArea.form;
        if (form === null) {
            return;
        }

        form.addEventListener("submit", ev => handleSubmit(form, vehiclesParent, vehiclesTextArea, ev));

        // add new-entry piece
        const newEntryContainer = document.createElement("div");
        newEntryContainer.classList.add("add-vehicle");
        vehiclesParent.appendChild(newEntryContainer);

        // disassemble text area
        const vehicleNumbers = vehiclesTextArea.value
            .split("\n")
            .map(vn => vn.trim())
            .filter(vn => vn.length > 0);
        for (let vehicleNumber of vehicleNumbers) {
            const vehicleNumberInput = addVehicle(vehiclesParent, newEntryContainer);
            vehicleNumberInput.value = vehicleNumber;
        }

        const plusButton = document.createElement("input");
        plusButton.type = "button";
        plusButton.value = "+";
        plusButton.addEventListener("click", () => {
            const newVehicleNumberInput = addVehicle(vehiclesParent, newEntryContainer);
            newVehicleNumberInput.focus();
        });
        newEntryContainer.appendChild(plusButton);

        vehiclesTextArea.style.display = "none";

        // focus company field
        const companySelect = <HTMLSelectElement|null>document.getElementById("bimdb-cae-company");
        if (companySelect !== null) {
            companySelect.focus();
        }
    }
}
