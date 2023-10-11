import { AddEdit } from './add_edit';
import { CouplingAddEdit } from './coupling_add_edit';

export module BimDatabase {
    export function setUpAddEdit() {
        document.addEventListener("DOMContentLoaded", AddEdit.doSetUp);
    }

    export function setUpCouplingAddEdit() {
        document.addEventListener("DOMContentLoaded", CouplingAddEdit.doSetUp);
    }
}

// "globals are evil"
declare global {
    interface Window { BimDatabase: any; }
}
window.BimDatabase = BimDatabase;
