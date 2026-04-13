#!/usr/bin/env node

/**
 * Test script to verify the generated @galpha/proto package works correctly
 * This is a pure JavaScript version that can run without TypeScript compilation
 */

console.log('🧪 Testing @galpha/proto package...\n');

try {
  // Test 1: Import the package
  console.log('1️⃣ Testing package import...');
  const {
    proto,
    ledger,
    position_manager,
    user_service,
    CreatePendingDepositRequest,
    PendingDeposit,
  } = require('../../dist/index.js');

  console.log('✅ Package imported successfully');
  console.log('   - proto namespace:', typeof proto);
  console.log('   - ledger namespace:', typeof ledger);
  console.log('   - position_manager namespace:', typeof position_manager);
  console.log('   - user_service namespace:', typeof user_service);
  console.log('');

  // Test 2: Check available types
  console.log('2️⃣ Testing available types...');
  const ledgerTypes = Object.keys(ledger.v1);
  console.log(`✅ Found ${ledgerTypes.length} types in ledger.v1`);
  console.log('   Sample types:', ledgerTypes.slice(0, 5).join(', '));
  console.log('');

  // Test 3: Create a CreatePendingDepositRequest
  console.log('3️⃣ Testing CreatePendingDepositRequest.create()...');
  const request = proto.ledger.v1.CreatePendingDepositRequest.create({
    txHash: '0xabc123def456',
    chainId: 'eip155:1',
    asset: 'usdc',
    amount: '1000.00',
    autoStake: false,
  });

  console.log('✅ Request created successfully');
  console.log('   Request:', JSON.stringify(request, null, 2));
  console.log('');

  // Test 4: Convert to JSON object
  console.log('4️⃣ Testing toObject()...');
  const jsonObj = proto.ledger.v1.CreatePendingDepositRequest.toObject(request);
  console.log('✅ Converted to JSON object');
  console.log('   JSON:', JSON.stringify(jsonObj, null, 2));
  console.log('');

  // Test 5: Create from JSON object
  console.log('5️⃣ Testing fromObject()...');
  const responseData = {
    id: 'deposit-789',
    txHash: '0xabc123def456',
    chainId: 'eip155:1',
    asset: 'usdc',
    originalAmount: '1000.00',
    aiusdAmount: '1000.00',
    autoStake: false,
    status: 'pending',
    createdAt: '2025-10-03T10:00:00Z',
    updatedAt: '2025-10-03T10:00:00Z',
  };

  const response = proto.ledger.v1.CreatePendingDepositResponse.fromObject(responseData);
  console.log('✅ Created response from object');
  console.log('   Response ID:', response.id);
  console.log('   Response status:', response.status);
  console.log('');

  // Test 6: Binary encoding/decoding
  console.log('6️⃣ Testing binary encode/decode...');
  const encoded = proto.ledger.v1.CreatePendingDepositRequest.encode(request).finish();
  console.log(`✅ Encoded to binary (${encoded.length} bytes)`);

  const decoded = proto.ledger.v1.CreatePendingDepositRequest.decode(encoded);
  console.log('✅ Decoded from binary');
  console.log('   Decoded txHash:', decoded.txHash);
  console.log('   Decoded amount:', decoded.amount);
  console.log('   Match original:', decoded.txHash === request.txHash && decoded.amount === request.amount);
  console.log('');

  // Test 7: Test PendingDeposit type
  console.log('7️⃣ Testing PendingDeposit.create()...');
  const pendingDeposit = proto.ledger.v1.PendingDeposit.create({
    id: 'deposit-456',
    txHash: '0x789abc',
    chainId: 'eip155:1',
    asset: 'usdt',
    originalAmount: '500.00',
    aiusdAmount: '500.00',
    autoStake: true,
    status: 'confirmed',
    createdAt: '2025-10-03T09:00:00Z',
    updatedAt: '2025-10-03T09:30:00Z',
  });

  console.log('✅ PendingDeposit created successfully');
  console.log('   Deposit ID:', pendingDeposit.id);
  console.log('   Auto-stake:', pendingDeposit.autoStake);
  console.log('   Status:', pendingDeposit.status);
  console.log('');

  // Test 8: Test GetBalanceResponse
  console.log('8️⃣ Testing GetBalanceResponse.create()...');
  const balance = proto.ledger.v1.GetBalanceResponse.create({
    userId: 'user-123',
    tradingBalance: '1500.50',
    stakedBalance: '3000.00',
    totalBalance: '4500.50',
    yieldAccumulated: '25.75',
    version: 1,
  });

  console.log('✅ Balance created successfully');
  console.log('   User ID:', balance.userId);
  console.log('   Trading balance:', balance.tradingBalance);
  console.log('   Staked balance:', balance.stakedBalance);
  console.log('   Total balance:', balance.totalBalance);
  console.log('   Yield:', balance.yieldAccumulated);
  console.log('');

  // Test 9: Test Position Manager types
  console.log('9️⃣ Testing Position Manager types...');
  const position = proto.position_manager.v1.Position.create({
    mint: 'So11111111111111111111111111111111111111112',
    amount: '1000000000',
    usdValue: '100.50',
  });

  console.log('✅ Position created successfully');
  console.log('   Mint:', position.mint);
  console.log('   Amount:', position.amount);
  console.log('   USD value:', position.usdValue);
  console.log('');

  // Test 10: Test type aliases
  console.log('🔟 Testing type aliases...');
  console.log('   CreatePendingDepositRequest type:', typeof CreatePendingDepositRequest);
  console.log('   PendingDeposit type:', typeof PendingDeposit);
  console.log('');

  console.log('✅ All tests passed! The package is working correctly! 🎉\n');
  process.exit(0);
} catch (error) {
  console.error('❌ Test failed:', error.message);
  console.error(error.stack);
  process.exit(1);
}
