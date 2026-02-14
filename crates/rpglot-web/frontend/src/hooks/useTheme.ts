import { useState, useEffect, useCallback } from "react";

type Theme = "light" | "dark" | "system";

function getEffective(theme: Theme): "light" | "dark" {
    if (theme !== "system") return theme;
    return window.matchMedia("(prefers-color-scheme: dark)").matches
        ? "dark"
        : "light";
}

export function useTheme() {
    const [theme, setThemeState] = useState<Theme>(() => {
        return (localStorage.getItem("rpglot-theme") as Theme) || "system";
    });

    const effective = getEffective(theme);

    const setTheme = useCallback((t: Theme) => {
        setThemeState(t);
        if (t === "system") {
            localStorage.removeItem("rpglot-theme");
        } else {
            localStorage.setItem("rpglot-theme", t);
        }
    }, []);

    useEffect(() => {
        const root = document.documentElement;
        if (effective === "dark") {
            root.classList.add("dark");
        } else {
            root.classList.remove("dark");
        }
    }, [effective]);

    // Listen for system theme changes
    useEffect(() => {
        if (theme !== "system") return;
        const mq = window.matchMedia("(prefers-color-scheme: dark)");
        const handler = () => setThemeState((prev) => (prev === "system" ? "system" : prev));
        mq.addEventListener("change", handler);
        return () => mq.removeEventListener("change", handler);
    }, [theme]);

    const cycle = useCallback(() => {
        setTheme(
            theme === "light" ? "dark" : theme === "dark" ? "system" : "light",
        );
    }, [theme, setTheme]);

    return { theme, effective, setTheme, cycle };
}
