function make_decrementer(balance) {
    return amount => balance - amount;
}
const D1 = make_decrementer(25);

const D2 = make_decrementer(25);
