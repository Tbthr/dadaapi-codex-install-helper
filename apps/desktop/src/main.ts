import "@fontsource-variable/instrument-sans";
import "@fontsource-variable/source-serif-4";
import { createPinia } from "pinia";
import { createApp } from "vue";
import App from "./App.vue";
import "./styles/app.css";

const app = createApp(App);
app.use(createPinia());
app.mount("#app");
