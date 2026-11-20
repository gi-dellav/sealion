import { head, is_pair, list, pair } from './sicp.js';
function is_tagged_list(component, the_tag) {
    return is_pair(component) && head(component) === the_tag;
}

is_tagged_list(list("name", "x"), "name");

// expected: true
