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
function make_account_and_serializer(balance) {
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
    const balance_serializer = make_serializer();
    return m => m === "withdraw"
                ? balance_serializer(withdraw)
                : m === "deposit"
                ? balance_serializer(deposit)
                : m === "balance"
                ? balance
                : m === "serializer"
                ? balance_serializer
                : error(m, "unknown request -- make_account");
}
function deposit(account, amount) {
    account("deposit")(amount);
}

const my_account = make_account_and_serializer(300);
deposit(my_account, 100);	
display(my_account("balance"));
