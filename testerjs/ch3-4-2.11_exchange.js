// chapter=3 variant=concurrent 
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
function exchange(account1, account2) {
    const difference = account1("balance") - account2("balance");
    account1("withdraw")(difference);
    account2("deposit")(difference);
}

const a1 = make_account(100);
const a2 = make_account(200);
const a3 = make_account(300);

concurrent_execute(
    () => { 
        display(a1("balance"), "Peter balance a1 before");
        display(a2("balance"), "Peter balance a2 before");
        exchange(a1, a2);
        display(a1("balance"), "Peter balance a1 after");
        display(a2("balance"), "Peter balance a2 after");
    },
    () => { 
        display(a1("balance"), "Paul balance a1 before");
        display(a3("balance"), "Paul balance a3 before");
        exchange(a1, a3);
        display(a1("balance"), "Paul balance a1 after");
        display(a3("balance"), "Paul balance a3 after");
    }
);
