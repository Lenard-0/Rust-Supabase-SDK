# Rust SupaBase SDK

## Overview

Use SupaBase in Rust with no stress & without worrying about low level implementation.

## Usage

To get started simply initialise a SupaBaseClient through SupaBaseClient::new(supabase_url, secret_key). Then start making requests and you're good to go! (Keep in mind secret key and service role are synonymous).

Most requests follow the same structure and require the (table name, id | search param, Option<body>)

Functionality currently supported:
- Get by ID
- Create (must use UUID)
- Update
- Upsert
- Delete
- Get all
- Search / Get with a query

If you want new features or improvements let me know through the Github:
https://github.com/Lenard-0/Rust-Supabase-SDK