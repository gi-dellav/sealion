const my_account = make_account(100);

concurrent_execute(
    () => {
        display(my_account("balance"), "T1 balance");
        my_account("withdraw")(50);
        display(my_account("balance"), "T1 balance");
        my_account("deposit")(100);
    },
    () => { display(my_account("balance"), "T2 balance");
        my_account("withdraw")(50);
        display(my_account("balance"), "T2 balance");
        my_account("deposit")(100);
    }
);
