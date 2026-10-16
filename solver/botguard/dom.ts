import { parseHTML } from "linkedom";

export function setupDom(url = "https://www.youtube.com/") {
	const dom = parseHTML(
		"<!DOCTYPE html><html><head></head><body></body></html>",
	);
	const { window, document } = dom;

	const locationObj = {
		href: url,
		origin: "https://www.youtube.com",
		protocol: "https:",
		host: "www.youtube.com",
		hostname: "www.youtube.com",
		port: "",
		pathname: "/",
		search: "",
		hash: "",
		assign: () => {},
		replace: () => {},
		reload: () => {},
	};

	const navigatorObj = {
		userAgent:
			"Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
		appVersion:
			"5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
		platform: "Win32",
		language: "en-US",
		languages: ["en-US", "en"],
		cookieEnabled: true,
		hardwareConcurrency: 8,
		webdriver: false,
		onLine: true,
	};

	const screenObj = {
		width: 1920,
		height: 1080,
		availWidth: 1920,
		availHeight: 1040,
		colorDepth: 24,
		pixelDepth: 24,
	};

	const origCreateElement = document.createElement.bind(document);
	document.createElement = (
		tagName: string,
		options?: ElementCreationOptions,
	) => {
		const el = origCreateElement(tagName, options);
		if (tagName.toLowerCase() === "canvas") {
			(el as unknown as Record<string, unknown>).getContext = () => null;
		}
		return el;
	};

	const g = globalThis as unknown as Record<string, unknown>;
	g.window = window;
	g.document = document;
	g.location = locationObj;
	g.origin = "https://www.youtube.com";
	g.navigator = navigatorObj;
	g.screen = screenObj;

	return {
		dom,
		release() {
			for (const prop of [
				"window",
				"document",
				"location",
				"origin",
				"navigator",
				"screen",
				"yt",
			]) {
				try {
					delete g[prop];
				} catch {
					g[prop] = undefined;
				}
			}
		},
	};
}
