//@ts-ignore
export class SpiceElement extends Element {

    #backend;

    #eventBinder;

    constructor() {
        // @ts-ignore
        const [el, backend] = SpiceBackend_new();
        super(el);
        this.#backend = backend;
        this.#eventBinder = this.createEventBinder(backend, SpiceBackend_bind_js_event_listener);
    }

    connect(uri) {
        SpiceBackend_connect(this.#backend, uri);
    }

    bindDisplayOpen(callback) {
        this.#eventBinder.bindEvent('displayopen', callback);
    }

    bindDisplayClose(callback) {
        this.#eventBinder.bindEvent('displayclose', callback);
    }

    bindConnectSuccess(callback) {
        this.#eventBinder.bindEvent("connectsuccess", callback);
    }

    bindConnectFail(callback) {
        this.#eventBinder.bindEvent("connectfail", callback);
    }

}

globalThis.SpiceElement = SpiceElement;