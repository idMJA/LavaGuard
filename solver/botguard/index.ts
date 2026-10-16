import { setupDom } from "./dom.ts";
import {
	buildURL,
	createColdStartToken,
	INNERTUBE_API_KEY,
	parse_json,
	REQUEST_KEY,
	TV_CONFIG,
	TV_USER_AGENT,
	toBase64,
	toUint8,
	USER_AGENT,
	utf8Decode,
	utf8Encode,
	YT_BASE,
} from "./helpers.ts";

export interface BotGuardChallenge {
	program: string;
	globalName: string;
	interpreterUrl?: string;
	interpreterJavascript?: string;
}

function parseWaaChallenge(rawData: unknown): BotGuardChallenge | null {
	const rawObj = rawData as Record<string, unknown>;
	if (rawObj?.bgChallenge) {
		const bg = rawObj.bgChallenge as Record<string, unknown>;
		return {
			program: bg.program as string,
			globalName: bg.globalName as string,
			interpreterUrl: (bg.interpreterUrl as Record<string, string>)
				?.privateDoNotAccessOrElseTrustedResourceUrlWrappedValue,
			interpreterJavascript: (
				bg.interpreterJavascript as Record<string, string>
			)?.privateDoNotAccessOrElseSafeScriptWrappedValue,
		};
	}

	const rawArr = rawData as unknown[];
	let challengeData: unknown[];

	if (rawArr?.length > 1 && typeof rawArr[1] === "string") {
		const bytes = toUint8(rawArr[1]);
		challengeData = JSON.parse(utf8Decode(bytes.map((value) => value + 97)));
	} else if (Array.isArray(rawArr?.[0])) {
		challengeData = rawArr[0];
	} else if (Array.isArray(rawData)) {
		challengeData = rawData;
	} else {
		return null;
	}

	if (!Array.isArray(challengeData)) return null;

	const [, wrappedScript, wrappedUrl, , program, globalName] = challengeData;
	const script = Array.isArray(wrappedScript)
		? wrappedScript.find((value) => typeof value === "string")
		: undefined;
	const interpreterUrl = Array.isArray(wrappedUrl)
		? wrappedUrl.find((value) => typeof value === "string")
		: undefined;

	if (!program || !globalName || (!interpreterUrl && !script)) return null;

	return {
		program: program as string,
		globalName: globalName as string,
		interpreterUrl: interpreterUrl as string | undefined,
		interpreterJavascript: script as string | undefined,
	};
}

async function fetchChallenge(): Promise<{
	challenge: BotGuardChallenge;
	key: string;
}> {
	let key = REQUEST_KEY;
	let challenge: BotGuardChallenge | null = null;
	let eacrToken: string | undefined;

	try {
		const res = await fetch(YT_BASE, {
			headers: {
				accept: "*/*",
				"accept-language": "en-US,en;q=0.7",
				"user-agent": USER_AGENT,
			},
		});
		const txt = await res.text();

		const attestation = txt.match(/window\.ytAtN\(\s*({[\s\S]*?})\s*\)/);
		if (attestation) {
			try {
				const parsed = parse_json(attestation[1]) as Record<string, unknown>;
				if (parsed && typeof parsed === "object") {
					challenge = (parsed as { bgChallenge?: unknown }).bgChallenge
						? (parsed as unknown as BotGuardChallenge)
						: parseWaaChallenge((parsed as { R?: unknown }).R);
					eacrToken = (parsed as { T?: string }).T;
				}
			} catch {}
		}

		if (!challenge && !eacrToken) {
			const initialAttestationMatch = txt.match(
				/initialAttestationDataJson\s*=\s*({[\s\S]*?});/,
			);
			if (initialAttestationMatch) {
				try {
					const parsed = parse_json(initialAttestationMatch[1]) as Record<
						string,
						unknown
					>;
					if (parsed && typeof parsed === "object") {
						challenge = (parsed as { bgChallenge?: unknown }).bgChallenge
							? (parsed as unknown as BotGuardChallenge)
							: parseWaaChallenge((parsed as { R?: unknown }).R);
						eacrToken = (parsed as { T?: string }).T || eacrToken;
					}
				} catch {}
			}
		}
	} catch {}

	if (!challenge) {
		try {
			const payload: Record<string, unknown> = {
				context: {
					client: {
						clientName: "WEB",
						clientVersion: "2.20260227.01.00",
					},
				},
				engagementType: "ENGAGEMENT_TYPE_UNBOUND",
			};
			if (eacrToken) payload.eacrToken = eacrToken;

			const attRes = await fetch(
				`${YT_BASE}/youtubei/v1/att/get?prettyPrint=false`,
				{
					method: "POST",
					headers: {
						accept: "*/*",
						"content-type": "application/json",
						"user-agent": USER_AGENT,
						"x-goog-api-key": INNERTUBE_API_KEY,
					},
					body: JSON.stringify(payload),
				},
			);

			if (attRes.ok) {
				const attestation = (await attRes.json()) as Record<string, unknown>;
				if (attestation?.bgChallenge) {
					challenge = parseWaaChallenge(attestation);
				}
			}
		} catch {}
	}

	if (!challenge) {
		try {
			const waaRes = await fetch(buildURL("Create", false), {
				method: "POST",
				headers: {
					"content-type": "application/json+protobuf",
					"x-goog-api-key": INNERTUBE_API_KEY,
					"x-user-agent": "grpc-web-javascript/0.1",
					"user-agent": USER_AGENT,
				},
				body: JSON.stringify([key]),
			});

			if (waaRes.ok) {
				challenge = parseWaaChallenge(await waaRes.json());
			}
		} catch {}
	}

	if (!challenge) {
		try {
			const tvRes = await fetch(TV_CONFIG, {
				headers: { accept: "*/*", "user-agent": TV_USER_AGENT },
			});
			const txt = await tvRes.text();
			if (txt.startsWith(")]}'")) {
				const json = JSON.parse(txt.slice(4));
				challenge = json.challengeParams?.R
					? (parseWaaChallenge(JSON.parse(json.challengeParams.R)) ??
						parseWaaChallenge(json.challengeParams.R))
					: null;
				key = json.challengeRequestKey || key;
			}
		} catch {}
	}

	if (!challenge) {
		throw new Error("failed obtaining botguard challenge from all fallbacks");
	}

	return { challenge, key };
}

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

		type VM = {
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

		const vm = bgGlobal as unknown as VM;
		if (!vm || typeof vm.a !== "function") {
			throw new Error("botguard VM function 'a' missing");
		}

		type AsyncSnapshotCallback = (
			cb: (res: unknown) => void,
			args: unknown[],
		) => void;
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
