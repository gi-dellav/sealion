// chapter=2 variant=lazy 
const xs = null;
// Source Academy opens this program
// in lazy mode. Choose "Source §2" to
// to compare with strict mode
function unless(condition, usual_value, exceptional_value) {      
    return condition ? exceptional_value : usual_value;
}

unless(is_null(xs), head(xs), display("error: xs should not be null"));
