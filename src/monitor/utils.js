HTMLElement.prototype.new_child = function(tagName, props, classes, text) {
    let element = document.createElement(tagName);

    if (text) element.innerText = text;

    for (let prop in props) {
        element[prop] = props[prop];
    }

    if (classes) {
        for (let i = 0; i < classes.length; i++) {
            element.classList.add(classes[i]);
        }
    }

    if (this !== undefined) {
        this.appendChild(element);
    }

    return element;
};

function object_eq(a, b) {
    let a_null = a === null;
    let b_null = b === null;
    let any_null = a_null || b_null;

    let a_nobj = (typeof a) !== 'object';
    let b_nobj = (typeof b) !== 'object';
    let any_nobj = a_nobj || b_nobj;

    if (any_null || any_nobj) return a === b;

    for (prop in a) {
        let x = a[prop], y = b[prop];
        if (!object_eq(x, y)) return false;
    }

    for (prop in b) {
        let x = a[prop], y = b[prop];
        if (typeof x !== typeof y) return false;
    }

    return true;
}

function find(id) {
    return document.getElementById(id);
}