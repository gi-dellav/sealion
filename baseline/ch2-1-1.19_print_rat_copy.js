import { display, stringify } from './sicp.js';
function print_rat(x) {
    return display(stringify(numer(x)) + " / " + stringify(denom(x)));
}
