import { head, set_head } from './sicp.js';
function set_to_wow(x) {
    set_head(head(x), "wow");
    return x;
}
