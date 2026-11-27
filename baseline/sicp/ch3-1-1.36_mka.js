function call_the_cops(reason) {
    return "calling the cops because " + reason;
}
function make_account(balance, p) {

    let invalid_attempts = 0; //initializes to 0

    function withdraw(amount) {
        if (balance >= amount) {
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

    function calling_the_cops(_) {
        return call_the_cops("you have exceeded " +
                             "the max no of failed attempts");
    }

    function dispatch(m, q) {
        if (invalid_attempts > 7) {
            return calling_the_cops;
        } else if (p === q) {
            invalid_attempts = 0; // reset after a correct password
            return m === "withdraw"
                   ? withdraw
                   : m === "deposit"
                   ? deposit
                   : "Unknown request: make_account";
        } else {
            invalid_attempts = invalid_attempts + 1;
            if (invalid_attempts > 7) {
                return calling_the_cops;
            } else {
                return x => "Incorrect Password";
            }
        }
    }
    
    return dispatch;

}

const a = make_account(100, "rosebud");
a("withdraw", "rosebad")(50);
a("withdraw", "rosebad")(50);
a("withdraw", "rosebad")(50);
a("withdraw", "rosebad")(50);
a("withdraw", "rosebad")(50);
a("withdraw", "rosebad")(50);
a("withdraw", "rosebad")(50);
a("withdraw", "rosebad")(50);

// expected: 'calling the cops because you have exceeded the max no of failed attempts'
