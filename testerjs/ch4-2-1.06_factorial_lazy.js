// chapter=2 variant=lazy 
// Source Academy opens this program
// in lazy mode. Choose "Source §2" to
// to compare with strict mode
function unless(condition, usual_value, exceptional_value) {      
    return condition ? exceptional_value : usual_value;
}
// Source Academy opens this program
// in lazy mode. Choose "Source §2" to
// to compare with strict mode

function factorial(n) {
    return unless(n === 1,
                  n * factorial(n - 1),
                  1);
}

factorial(5);
