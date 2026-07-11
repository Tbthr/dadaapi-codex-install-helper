import "@fontsource-variable/instrument-sans";
import "@fontsource-variable/source-serif-4";
import { createPinia } from "pinia";
import { createApp } from "vue";
import App from "./App.vue";
import { useThemeStore } from "./stores/theme";
import "./styles/app.css";

const app = createApp(App);
const pinia = createPinia();
app.use(pinia);
useThemeStore(pinia).initialize();
app.mount("#app");
