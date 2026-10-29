function make_decrementer(balance) {
    return amount => balance - amount;
}
const D = make_decrementer(25);
