export {};
namespace N { export function first() { second(); } function privateName() {} }
namespace N { export function second() {} }
N.first(); N.second(); N.privateName();