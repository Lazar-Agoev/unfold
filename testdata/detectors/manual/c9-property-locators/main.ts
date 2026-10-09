const api = { arrow: () => {}, method() {} };
api.arrow(); api.method();
interface Api { arrow(): void }
const typed: Api = { arrow: () => {} };
typed.arrow();
const written = { arrow: () => {} };
written.arrow = () => {};
written.arrow();
