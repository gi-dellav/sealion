function make_mutex() {
    const cell = list(false);
    function the_mutex(m) {
        return m === "acquire"
               ? test_and_set(cell)
                 ? the_mutex("acquire") // retry
                 : true
               : m === "release"
               ? clear(cell)
               : error(m, "unknown request -- mutex");
    }
    return the_mutex;
}
function clear(cell) {
    set_head(cell, false);
}
function make_serializer() {
    const mutex = make_mutex();
    return f => {
               function serialized_f() {
                   mutex("acquire");
                   const val = f();
                   mutex("release");
                   return val;
               }
               return serialized_f;          
           };
}
function make_account(balance) {
    function withdraw(amount) {
        if (balance > amount) {
            balance = balance - amount;
            return balance;
        } else {
            return "Insufficient funds";
        }
    }
    function deposit(amount) {
        balance = balance + amount;
        return balance;
    }
    const protect = make_serializer();
    function dispatch(m) {
        return m === "withdraw" 
               ? protect(withdraw)
               : m === "deposit"
               ? protect(deposit)
               : m === "balance"
               ? balance
               : error(m, "unknown request -- make_account");
    }
    return dispatch;
}
const a1 = make_account(400);
const a2 = make_account(100);

concurrent_execute(
    () => {
        display(a1("balance"), "Peter balance a1 before");
        display(a2("balance"), "Peter balance a2 before");
        transfer(a1, a2, 50);
        display(a1("balance"), "Peter balance a1 after");
        display(a2("balance"), "Peter balance a2 after");
    },
    () => { 
        display(a1("balance"), "Paul balance a1");
        display(a2("balance"), "Paul balance a2");
        display(a1("balance"), "Paul balance a1");
        display(a2("balance"), "Paul balance a2");
        display(a1("balance"), "Paul balance a1");
        display(a2("balance"), "Paul balance a2");
    }
);
