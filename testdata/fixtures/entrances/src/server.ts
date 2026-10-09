import express from "express";
import { createOrder } from "./orders.js";

const app = express();
app.post("/orders", createOrder);
app.listen(3000);
