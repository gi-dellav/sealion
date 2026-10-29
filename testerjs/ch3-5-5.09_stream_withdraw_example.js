const my_amounts = list_to_stream(list(50, 100, 40));
const my_account_stream = stream_withdraw(200, my_amounts);	
stream_ref(my_account_stream, 2);
