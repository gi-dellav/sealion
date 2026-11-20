import { head, list, list_ref } from './sicp.js';
const the_global_environment = setup_environment();

list_ref(head(head(the_global_environment)), 12);
