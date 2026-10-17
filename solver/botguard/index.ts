import { fetchChallenge } from "./challenge.ts";
import { setupDom } from "./dom.ts";
import {
	buildURL,
	createColdStartToken,
	INNERTUBE_API_KEY,
	toBase64,
	toUint8,
	USER_AGENT,
	utf8Encode,
} from "./helpers.ts";

export type { BotGuardChallenge } from "./challenge.ts";
export { fetchChallenge, parseWaaChallenge } from "./challenge.ts";

type AsyncSnapshotCallback = (
	cb: (res: unknown) => void,
	args: unknown[],
) => void;

type BotGuardVM = {
	a: (
		program: string,
		callback: (
			async_snapshot: unknown,
			shutdown: unknown,
			pass_event: unknown,
			check_camera: unknown,
		) => void,
		flag: boolean,
		element: unknown,
		emptyFn: () => void,
		arrays: [unknown[], unknown[]],
		paramUndef: undefined,
		paramFalse: boolean,
		emptyArr: unknown[],
	) => Promise<[(args: unknown[]) => unknown]>;
};

export async function generatePoToken(
	contentBinding: string,
	coldToken = false,
) {
	const domSession = setupDom();

	try {
		const { challenge, key } = await fetchChallenge();

		let interpreter = challenge.interpreterJavascript;
		if (!interpreter && challenge.interpreterUrl) {
			const url = challenge.interpreterUrl.startsWith("http")
				? challenge.interpreterUrl
				: `https:${challenge.interpreterUrl}`;
			const res = await fetch(url);
			interpreter = await res.text();
		}

		if (!interpreter) {
			throw new Error("could not load botguard interpreter script");
		}

		const runInterpreter = new Function("code", "return (0, eval)(code);");
		runInterpreter(interpreter);

		const g = globalThis as Record<string, unknown>;
		const bgGlobal = g[challenge.globalName];
		if (!bgGlobal) {
			throw new Error(`botguard global '${challenge.globalName}' unavailable`);
		}

		const vm = bgGlobal as unknown as BotGuardVM;
		if (!vm || typeof vm.a !== "function") {
			throw new Error("botguard VM function 'a' missing");
		}

		let asyncSnapshotFn: AsyncSnapshotCallback | null = null;

		const syncSnapshot = (
			await vm.a(
				challenge.program,
				(snap) => {
					if (typeof snap === "function") {
						asyncSnapshotFn = snap as AsyncSnapshotCallback;
					}
				},
				true,
				undefined,
				() => {},
				[[], []],
				undefined,
				false,
				[],
			)
		)?.[0];

		const signals: unknown[] = [];
		const snapshotArgs = [contentBinding, undefined, signals, true];

		let botguardResponse: unknown;
		if (asyncSnapshotFn) {
			botguardResponse = await new Promise((resolve, reject) => {
				const timer = setTimeout(
					() => reject(new Error("VM snapshot timed out")),
					3000,
				);
				(asyncSnapshotFn as AsyncSnapshotCallback)((res: unknown) => {
					clearTimeout(timer);
					resolve(res);
				}, snapshotArgs);
			});
		} else if (typeof syncSnapshot === "function") {
			botguardResponse = syncSnapshot(snapshotArgs);
		} else {
			throw new Error("could not obtain botguard snapshot function");
		}

		const generateItUrl = buildURL("GenerateIT", true);
		const itRes = await fetch(generateItUrl, {
			method: "POST",
			headers: {
				"content-type": "application/json+protobuf",
				"x-goog-api-key": INNERTUBE_API_KEY,
				"x-user-agent": "grpc-web-javascript/0.1",
				"user-agent": USER_AGENT,
			},
			body: JSON.stringify([key, botguardResponse]),
		});

		if (!itRes.ok) {
			throw new Error(`GenerateIT returned HTTP ${itRes.status}`);
		}

		const [
			integrityToken,
			estimatedTtlSecs,
			_mintRefreshThreshold,
			websafeFallbackToken,
		] = (await itRes.json()) as [string, number, number, string];

		const getMinter = signals[0] as
			| ((
					token: Uint8Array,
			  ) => Promise<(binding: Uint8Array) => Promise<Uint8Array>>)
			| undefined;

		let poToken = "";
		if (typeof getMinter === "function" && integrityToken) {
			const minterFn = await getMinter(toUint8(integrityToken));
			if (typeof minterFn === "function") {
				const mintedBytes = await minterFn(utf8Encode(contentBinding));
				poToken = toBase64(mintedBytes, true);
			}
		}

		if (!poToken && websafeFallbackToken) {
			poToken = websafeFallbackToken;
		}

		if (!poToken) {
			throw new Error("failed producing webPoSignalOutput or fallback token");
		}

		return {
			poToken,
			contentBinding,
			coldStartToken: coldToken
				? createColdStartToken(contentBinding)
				: undefined,
			ttl: Number(estimatedTtlSecs) || 21600,
		};
	} finally {
		domSession.release();
	}
}
