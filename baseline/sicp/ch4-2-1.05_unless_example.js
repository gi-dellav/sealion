import { display, error, head, is_null } from './sicp.js';
// chapter=2 variant=lazy 
// Source Academy opens this program
// in lazy mode. Choose "Source §2" to
// to compare with strict mode
function unless(condition, usual_value, exceptional_value) {      
    return condition ? exceptional_value : usual_value;
}
const xs = null;
unless(is_null(xs), head(xs), display("error: xs should not be null"));
