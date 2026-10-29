let state = 2;

function rand(symbol) {
    if (symbol === "reset") {
        return new_state => {
                   state = new_state;
               };
    } else {
        // symbol is "generate"
        state = (state * 1010) % 1101;
        return state;
    } 
}

rand("generate");
rand("generate");
rand("generate");

// This generates 919, 47, 127

rand("reset")(2);

// State is reset to 2 again

rand("generate");
rand("generate");
rand("generate");

// Because initial state is the same, 919, 47, 127 is generated

// expected: 127
