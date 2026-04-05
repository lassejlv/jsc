// @ts-nocheck
import { Hono } from "hono";

const app = new Hono();

app.get("/", (c) => c.text("Hey!"));

JSC.serve({ port: 3000, fetch: app.fetch });
